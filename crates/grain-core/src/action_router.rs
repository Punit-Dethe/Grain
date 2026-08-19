//! [GRAIN] Tier L — the lexical leg of action routing (`docs/Action Routing/PLAN.md` §4.1).
//!
//! **This is the floor, not a fast path.** Grain's embedder is a ~130 MB opt-in
//! download that is not shipped, so a router that needs it is a router most
//! users do not have. Everything here runs with no model, no allocation beyond
//! the query, and no dependency on anything outside `grain-sdk`.
//!
//! Three jobs, all pure functions so the eval harness can drive them without a
//! running app:
//!
//! 1. **Score** what was said against every installed action's declared
//!    utterances.
//! 2. **Group** actions that are the same request handled by different
//!    extensions, so a second music extension does not turn every media command
//!    into a chooser (REVIEW-PASS2 D1).
//! 3. **Extract** the span behind each `{param}`, leaving resolution to the
//!    extension that owns the catalogue.
//!
//! What is deliberately NOT here: the decision thresholds, the outcome enum, the
//! session, and execution. Those are P1, and keeping them out means this file
//! can be measured before any of it exists.

use grain_sdk::manifest::{parse_utterance, ActionDecl, ActionRisk, UtterancePart};
use std::collections::HashMap;

/// One installed action, flattened out of its extension for ranking.
///
/// Ranking is **global** — every installed action competes against every other
/// — which is exactly why an extension cannot own a phrase, and equally why a
/// greedy declaration degrades everyone. The ceilings in the manifest contract
/// are the other half of that bargain.
#[derive(Clone, Debug)]
pub struct IndexedAction {
    pub extension_id: String,
    pub action_id: String,
    pub domain: String,
    /// The permission-sheet line, carried so the chooser and the read-back can
    /// name the action without going back to the manifest on the felt path.
    pub title: String,
    /// Whether performing this needs a read-back. Carried here so the decision
    /// layer never has to look it up — an action that reaches `Execute` with the
    /// wrong risk is the one bug in this feature with no recovery.
    pub risk: ActionRisk,
    /// Names of parameters that must be filled before this can run. An empty
    /// span for one of these is the chooser's business, not a guess.
    pub required_params: Vec<String>,
    /// Pre-parsed templates, in declaration order. Parsed once at index build:
    /// the router must never parse a template on the felt path.
    pub templates: Vec<Vec<UtterancePart>>,
}

impl IndexedAction {
    /// Flatten one declaration. Utterances that fail to parse are dropped rather
    /// than failing the build — validation already rejected them at import, so
    /// reaching here means a hand-edited registry, and a partly-indexed action
    /// is better than an unusable one.
    pub fn from_decl(extension_id: &str, decl: &ActionDecl) -> Self {
        IndexedAction {
            extension_id: extension_id.to_string(),
            action_id: decl.id.trim().to_string(),
            domain: decl.domain.trim().to_string(),
            title: decl.title.trim().to_string(),
            risk: decl.risk,
            required_params: decl
                .params
                .iter()
                .filter(|p| p.required)
                .map(|p| p.name.trim().to_string())
                .collect(),
            templates: decl
                .utterances
                .iter()
                .filter_map(|u| parse_utterance(u.trim()).ok())
                .collect(),
        }
    }

    /// `<extension>:<action>` — the id that appears in the action log.
    pub fn qualified(&self) -> String {
        format!("{}:{}", self.extension_id, self.action_id)
    }
}

/// How well one action matched, and by which route.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum MatchKind {
    /// Token-for-token overlap only. The weakest evidence.
    Overlap,
    /// Every literal in the template appeared, in order, with spans between.
    Template,
    /// The whole utterance is a declared phrase, verbatim after normalisation.
    Exact,
}

/// One scored candidate.
#[derive(Clone, Debug)]
pub struct Candidate {
    pub extension_id: String,
    pub action_id: String,
    pub domain: String,
    /// 0.0–1.0. Comparable across actions **within** Tier L only; the semantic
    /// leg produces its own scale and the two are combined, never concatenated.
    pub score: f32,
    pub kind: MatchKind,
    /// Spans captured for each `{param}`, keyed by parameter name. Raw: whatever
    /// the acoustic model produced, trimmed. Resolution belongs to the
    /// extension, which is the only party that knows its own catalogue.
    pub spans: HashMap<String, String>,
}

/// Lowercase, strip punctuation, collapse whitespace.
///
/// Deliberately crude. This runs on ASR output, which arrives without reliable
/// punctuation anyway, and every transformation here is one more place the
/// declared utterance and the spoken one can disagree.
pub fn normalise(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut pending_space = false;
    for c in text.chars() {
        if c.is_alphanumeric() {
            if pending_space && !out.is_empty() {
                out.push(' ');
            }
            pending_space = false;
            out.extend(c.to_lowercase());
        } else {
            pending_space = true;
        }
    }
    out
}

fn tokens(text: &str) -> Vec<&str> {
    text.split(' ').filter(|t| !t.is_empty()).collect()
}

/// Do two tokens count as the same word, allowing for what the acoustic model
/// does to short common words?
///
/// ASR substitutes rather than omits — "skip" arrives as "skit", "next" as
/// "nex" — and those substitutions pass every grammar check, which is what makes
/// them dangerous. Exact token equality throws the whole utterance away for one
/// mangled character; production voice systems all sit somewhere on this
/// spectrum, with Apple's phonetically-augmented rescoring at the far end.
///
/// This is the cheap end deliberately: bounded edit distance, no phonetic table,
/// no dependency. It is a **recall** aid only — the risk it adds is absorbed by
/// the conformal decision layer, which is why that had to exist first. Phonetic
/// keying (Double Metaphone) is the next rung and is named, not built.
fn same_word(a: &str, b: &str) -> bool {
    if a == b {
        return true;
    }
    // Short words are where a one-character edit changes the meaning entirely
    // ("on"/"in", "to"/"do"), so they get no tolerance at all.
    let (short, long) = if a.len() <= b.len() { (a, b) } else { (b, a) };
    if short.len() < 4 || long.len() - short.len() > 1 {
        return false;
    }
    edit_distance_at_most_one(short, long)
}

/// True when `a` becomes `b` with at most one insertion, deletion or
/// substitution. Bounded at one on purpose — two edits on a four-letter word is
/// a different word.
fn edit_distance_at_most_one(a: &str, b: &str) -> bool {
    let (a, b) = (a.as_bytes(), b.as_bytes());
    if a.len() == b.len() {
        let mut differences = 0;
        for i in 0..a.len() {
            if a[i] != b[i] {
                differences += 1;
                if differences > 1 {
                    return false;
                }
            }
        }
        return differences == 1;
    }
    // Exactly one insertion: walk both, allowing a single skip in the longer.
    let (mut i, mut j, mut skipped) = (0usize, 0usize, false);
    while i < a.len() && j < b.len() {
        if a[i] == b[j] {
            i += 1;
            j += 1;
        } else if skipped {
            return false;
        } else {
            skipped = true;
            j += 1;
        }
    }
    true
}

/// Score one utterance against one action, returning the best template's result.
pub fn score_action(spoken: &str, action: &IndexedAction) -> Option<Candidate> {
    let query = normalise(spoken);
    if query.is_empty() {
        return None;
    }
    let query_tokens = tokens(&query);
    let mut best: Option<(f32, MatchKind, HashMap<String, String>)> = None;

    for template in &action.templates {
        let scored = score_template(&query, &query_tokens, template);
        if let Some(candidate) = scored {
            let better = best
                .as_ref()
                .is_none_or(|(score, kind, _)| (candidate.1, candidate.0) > (*kind, *score));
            if better {
                best = Some(candidate);
            }
        }
    }

    let (score, kind, spans) = best?;
    Some(Candidate {
        extension_id: action.extension_id.clone(),
        action_id: action.action_id.clone(),
        domain: action.domain.clone(),
        score,
        kind,
        spans,
    })
}

/// Match one template. Literals must appear **in order**; whatever falls between
/// two literals is the span for the placeholder between them.
fn score_template(
    query: &str,
    query_tokens: &[&str],
    template: &[UtterancePart],
) -> Option<(f32, MatchKind, HashMap<String, String>)> {
    let literals: Vec<&str> = template
        .iter()
        .filter_map(|p| match p {
            UtterancePart::Literal(l) => Some(l.as_str()),
            UtterancePart::Param(_) => None,
        })
        .collect();
    let has_params = template
        .iter()
        .any(|p| matches!(p, UtterancePart::Param(_)));

    // No placeholders: the template is a phrase, so this is exact-or-overlap.
    if !has_params {
        let phrase = normalise(&literals.join(" "));
        if phrase == query {
            return Some((1.0, MatchKind::Exact, HashMap::new()));
        }
        let overlap = token_overlap(query_tokens, &phrase);
        // A single shared token is noise, not evidence — "play" overlapping
        // "play something else" should not put that action on the ballot.
        return (overlap >= 0.5).then(|| (overlap, MatchKind::Overlap, HashMap::new()));
    }

    // With placeholders: walk the template, consuming the query left to right.
    let mut spans: HashMap<String, String> = HashMap::new();
    let mut cursor = 0usize;
    let mut matched_literal_tokens = 0usize;
    let mut pending: Option<&str> = None;
    let starts_with_literal = matches!(template.first(), Some(UtterancePart::Literal(_)));
    let mut leading_junk = false;

    for part in template {
        match part {
            UtterancePart::Literal(literal) => {
                let needle = normalise(literal);
                if needle.is_empty() {
                    continue;
                }
                let found = find_token_aligned(&query[cursor..], &needle)?;
                let absolute = cursor + found;
                // A template that begins with a literal expects the utterance to
                // begin there too. Anything in front is unexplained by both the
                // literals and the spans, which is weaker evidence than a clean
                // match — "I was going to tell Jack that…" is not the command.
                if starts_with_literal && matched_literal_tokens == 0 && absolute > 0 {
                    leading_junk = true;
                }
                if let Some(name) = pending.take() {
                    let span = query[cursor..absolute].trim();
                    // A placeholder with nothing in front of the next literal
                    // has no value; a required one sends this to the chooser
                    // rather than being invented here.
                    if !span.is_empty() {
                        spans.insert(name.to_string(), span.to_string());
                    }
                }
                matched_literal_tokens += tokens(&needle).len();
                cursor = absolute + needle.len();
            }
            UtterancePart::Param(name) => {
                // Two placeholders in a row cannot be split without a literal
                // between them; the manifest allows it, so take the first
                // greedily rather than guessing a boundary.
                if let Some(previous) = pending.take() {
                    spans.insert(previous.to_string(), String::new());
                }
                pending = Some(name);
            }
        }
    }
    if let Some(name) = pending {
        let span = query[cursor..].trim();
        if !span.is_empty() {
            spans.insert(name.to_string(), span.to_string());
        }
    }

    if matched_literal_tokens == 0 {
        return None;
    }
    // The literals are the evidence; the spans are the payload — so the score
    // must not depend on how long the payload is.
    //
    // The first version divided matched literals by the QUERY length, which made
    // "tell Jack that I'm running late" score lower than "tell Jack that hi" for
    // no reason except that the message was longer. Worse, calibration examples
    // have short fillers, so real requests scored systematically below the bar
    // they were measured against and got refused.
    //
    // Score on the template's own specificity instead: more literal tokens
    // confirmed, in order, is stronger evidence, regardless of what sat between
    // them. Capped below an exact match, which is always the stronger claim.
    let specificity = (matched_literal_tokens as f32 / SPECIFICITY_SATURATION).min(1.0);
    let mut score = TEMPLATE_FLOOR + (TEMPLATE_CEILING - TEMPLATE_FLOOR) * specificity;
    if leading_junk {
        score -= LEADING_JUNK_PENALTY;
    }
    let _ = query_tokens;
    Some((score, MatchKind::Template, spans))
}

/// Literal tokens at which a template counts as fully specific. Three is where
/// "play my {x} playlist" sits and "play {x}" does not, which is the distinction
/// that matters.
const SPECIFICITY_SATURATION: f32 = 3.0;
/// A one-literal template ("play {artist}") — real evidence, weak evidence.
const TEMPLATE_FLOOR: f32 = 0.65;
/// A fully specific template. Below 1.0 on purpose: an exact declared phrase is
/// always the stronger claim.
const TEMPLATE_CEILING: f32 = 0.95;
/// Charged when a template that begins with a literal did not begin the
/// utterance.
const LEADING_JUNK_PENALTY: f32 = 0.15;

/// Find `needle` in `haystack` at token boundaries, so "on" does not match
/// inside "song".
fn find_token_aligned(haystack: &str, needle: &str) -> Option<usize> {
    let mut from = 0usize;
    while let Some(offset) = haystack[from..].find(needle) {
        let start = from + offset;
        let end = start + needle.len();
        let left_ok = start == 0 || haystack.as_bytes()[start - 1] == b' ';
        let right_ok = end == haystack.len() || haystack.as_bytes()[end] == b' ';
        if left_ok && right_ok {
            return Some(start);
        }
        from = start + 1;
        if from >= haystack.len() {
            break;
        }
    }
    None
}

/// Fraction of the phrase's tokens present in the query, penalised by how much
/// of the query is left unexplained.
fn token_overlap(query_tokens: &[&str], phrase: &str) -> f32 {
    let phrase_tokens = tokens(phrase);
    if phrase_tokens.is_empty() {
        return 0.0;
    }
    let hits = phrase_tokens
        .iter()
        .filter(|t| query_tokens.iter().any(|q| same_word(q, t)))
        .count();
    let recall = hits as f32 / phrase_tokens.len() as f32;
    let precision = hits as f32 / query_tokens.len().max(1) as f32;
    // Harmonic mean: a phrase fully contained in a much longer utterance is not
    // strong evidence, and neither is one token of a long phrase.
    if recall + precision == 0.0 {
        0.0
    } else {
        2.0 * recall * precision / (recall + precision)
    }
}

/// Score every indexed action, best first.
pub fn rank(spoken: &str, actions: &[IndexedAction]) -> Vec<Candidate> {
    let mut out: Vec<Candidate> = actions
        .iter()
        .filter_map(|action| score_action(spoken, action))
        .collect();
    out.sort_by(|a, b| {
        b.kind
            .cmp(&a.kind)
            .then(b.score.total_cmp(&a.score))
            // Stable, and independent of registry order: two candidates that
            // tie on evidence must not depend on install order for which one
            // the chooser lists first.
            .then(a.extension_id.cmp(&b.extension_id))
            .then(a.action_id.cmp(&b.action_id))
    });
    out
}

// ── Equivalence classes (REVIEW-PASS2 D1) ───────────────────────────────────

/// Grouping of actions that are **the same request handled by different
/// extensions**.
///
/// Without this, Spotify's `next` and Apple Music's `next` are rivals with
/// near-identical utterances, the top-two margin is permanently zero, and every
/// media command falls to the chooser — so the provider ladder, which exists for
/// exactly that case, never runs.
///
/// Computed, never declared. The alternative is a canonical verb space, which
/// is the design four platforms tried and three abandoned (RESEARCH §2).
#[derive(Clone, Debug, Default)]
pub struct EquivalenceMap {
    /// `<extension>:<action>` → class index.
    of: HashMap<String, usize>,
    count: usize,
}

impl EquivalenceMap {
    pub fn class_of(&self, extension_id: &str, action_id: &str) -> Option<usize> {
        self.of.get(&format!("{extension_id}:{action_id}")).copied()
    }

    pub fn len(&self) -> usize {
        self.count
    }

    pub fn is_empty(&self) -> bool {
        self.count == 0
    }
}

/// How much of two actions' declared language must coincide before they are
/// treated as the same request.
const EQUIVALENCE_THRESHOLD: f32 = 0.5;

/// How closely two individual phrases must match to count as the same phrasing.
/// Higher than the set threshold: a short generic phrase overlapping a longer
/// specific one is not evidence that two actions do the same thing.
const PHRASE_MATCH_THRESHOLD: f32 = 0.8;

/// Build equivalence classes over the installed set — **constrained
/// complete-linkage agglomerative clustering**.
///
/// # Why not union-find
///
/// The first implementation linked any two actions above the threshold and
/// unioned them, which is *single-linkage* clustering. Single linkage has a
/// textbook failure mode — the **chaining effect**: two well-separated clusters
/// joined by one bridging pair are merged into a long thin cluster whose ends
/// have nothing to do with each other. Both bugs the golden set found were that
/// one failure wearing different hats:
///
/// - Apple's `"play"` bridged Spotify's `"play my playlist"` and Spotify's
///   `play_artist`, so "play my gym playlist" became a provider chooser;
/// - `A(ext1) ~ B(ext2) ~ C(ext1)` collapsed one author's own two actions
///   through a third party, silently making one unreachable.
///
/// Complete linkage merges two clusters only when **every** cross-pair is
/// similar enough, so a single bridging pair can never chain, and both bugs are
/// structurally impossible rather than patched. It is also the standard fix for
/// exactly this, and the sizes involved here (hundreds of actions, once per
/// index rebuild) make the extra comparisons free.
///
/// # The constraint
///
/// Two actions of the **same extension** carry a hard cannot-link: an author's
/// own vocabulary is theirs to keep distinct, and merging two of their actions
/// makes one permanently unreachable. Under complete linkage a cannot-link pair
/// blocks any merge that would co-locate them, so the constraint holds
/// transitively for free — which is the property the previous version had to
/// enforce by hand.
///
/// Actions in different domains are never compared: "next slide" and "next
/// track" are precisely what must stay apart.
pub fn equivalence_classes(actions: &[IndexedAction]) -> EquivalenceMap {
    let n = actions.len();
    let phrases: Vec<Vec<String>> = actions.iter().map(literal_phrases).collect();

    // Pairwise similarity, with `None` for a cannot-link pair. Complete linkage
    // reads a cannot-link as "-infinity", which blocks the merge outright.
    let mut similarity = vec![vec![None::<f32>; n]; n];
    for a in 0..n {
        for b in (a + 1)..n {
            let comparable = actions[a].domain == actions[b].domain
                && actions[a].extension_id != actions[b].extension_id;
            let value = comparable.then(|| phrase_set_similarity(&phrases[a], &phrases[b]));
            similarity[a][b] = value;
            similarity[b][a] = value;
        }
    }

    let mut clusters: Vec<Vec<usize>> = (0..n).map(|i| vec![i]).collect();
    loop {
        // Complete linkage between two clusters = their WEAKEST cross-pair.
        let mut best: Option<(usize, usize, f32)> = None;
        for i in 0..clusters.len() {
            for j in (i + 1)..clusters.len() {
                let mut weakest = f32::INFINITY;
                let mut blocked = false;
                for a in &clusters[i] {
                    for b in &clusters[j] {
                        match similarity[*a][*b] {
                            // A single cannot-link anywhere across the two
                            // clusters vetoes the whole merge.
                            None => blocked = true,
                            Some(value) => weakest = weakest.min(value),
                        }
                    }
                }
                if blocked || weakest < EQUIVALENCE_THRESHOLD {
                    continue;
                }
                if best.is_none_or(|(_, _, current)| weakest > current) {
                    best = Some((i, j, weakest));
                }
            }
        }
        let Some((i, j, _)) = best else { break };
        let merged = clusters.remove(j);
        clusters[i].extend(merged);
    }

    let mut of = HashMap::new();
    for (class, members) in clusters.iter().enumerate() {
        for member in members {
            of.insert(actions[*member].qualified(), class);
        }
    }
    EquivalenceMap {
        of,
        count: clusters.len(),
    }
}

/// The literal (non-placeholder) text of each template, normalised. Placeholders
/// are dropped: `play {artist}` and `play {track}` are the same *request shape*
/// even though the authors named their parameters differently.
fn literal_phrases(action: &IndexedAction) -> Vec<String> {
    action
        .templates
        .iter()
        .map(|template| {
            let literals: Vec<&str> = template
                .iter()
                .filter_map(|p| match p {
                    UtterancePart::Literal(l) => Some(l.as_str()),
                    UtterancePart::Param(_) => None,
                })
                .collect();
            normalise(&literals.join(" "))
        })
        .filter(|p| !p.is_empty())
        .collect()
}

/// Best-match Jaccard over two phrase sets: how much of the smaller set has a
/// close counterpart in the larger.
fn phrase_set_similarity(a: &[String], b: &[String]) -> f32 {
    if a.is_empty() || b.is_empty() {
        return 0.0;
    }
    let matched = a
        .iter()
        .filter(|left| {
            b.iter().any(|right| {
                left == &right || token_overlap(&tokens(left), right) >= PHRASE_MATCH_THRESHOLD
            })
        })
        .count();
    matched as f32 / a.len().min(b.len()) as f32
}

// ── Author diagnostics (`grain-ext doctor`) ─────────────────────────────────

/// Two actions whose language overlaps enough that the router will struggle.
#[derive(Clone, Debug, PartialEq)]
pub struct Collision {
    pub left: String,
    pub right: String,
    pub shared: String,
    /// True when both sit in the same domain in different extensions — i.e. this
    /// is the *expected* kind of overlap, resolved by provider selection rather
    /// than being a mistake.
    pub is_provider_overlap: bool,
}

/// Find declarations that will fight at ranking time.
///
/// The author sees this in `doctor`, before users do. That is the whole point:
/// ranking is global, so a phrase that eats a neighbour's language is not a
/// problem the author would ever discover from their own extension in isolation.
pub fn collisions(actions: &[IndexedAction]) -> Vec<Collision> {
    let phrases: Vec<Vec<String>> = actions.iter().map(literal_phrases).collect();
    let mut out = Vec::new();
    for a in 0..actions.len() {
        for b in (a + 1)..actions.len() {
            let shared = phrases[a].iter().find(|left| {
                phrases[b]
                    .iter()
                    .any(|right| left == &right || token_overlap(&tokens(left), right) >= 0.8)
            });
            if let Some(shared) = shared {
                out.push(Collision {
                    left: actions[a].qualified(),
                    right: actions[b].qualified(),
                    shared: shared.clone(),
                    is_provider_overlap: actions[a].domain == actions[b].domain
                        && actions[a].extension_id != actions[b].extension_id,
                });
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use grain_sdk::manifest::{ActionParamDecl, ActionParamKind, ActionRisk, LayerWhen};

    fn decl(id: &str, domain: &str, utterances: &[&str], params: &[&str]) -> ActionDecl {
        ActionDecl {
            id: id.into(),
            title: id.into(),
            domain: domain.into(),
            risk: ActionRisk::Safe,
            when: LayerWhen::default(),
            utterances: utterances.iter().map(|u| (*u).to_string()).collect(),
            params: params
                .iter()
                .map(|name| ActionParamDecl {
                    name: (*name).into(),
                    kind: ActionParamKind::Entity,
                    resolve: true,
                    required: true,
                })
                .collect(),
            agent_rules: None,
        }
    }

    fn indexed(ext: &str, id: &str, domain: &str, utterances: &[&str]) -> IndexedAction {
        IndexedAction::from_decl(ext, &decl(id, domain, utterances, &[]))
    }

    fn with_params(
        ext: &str,
        id: &str,
        domain: &str,
        utterances: &[&str],
        params: &[&str],
    ) -> IndexedAction {
        IndexedAction::from_decl(ext, &decl(id, domain, utterances, params))
    }

    #[test]
    fn normalisation_survives_what_asr_actually_produces() {
        // No reliable punctuation, inconsistent case, stray spacing.
        assert_eq!(normalise("Skip this, please."), "skip this please");
        assert_eq!(normalise("  NEXT   song  "), "next song");
        assert_eq!(normalise("!!!"), "");
    }

    #[test]
    fn a_declared_phrase_matches_verbatim() {
        let next = indexed("spotify", "next", "media", &["skip this", "next song"]);
        let hit = score_action("Skip this.", &next).unwrap();
        assert_eq!(hit.kind, MatchKind::Exact);
        assert_eq!(hit.score, 1.0);
    }

    #[test]
    fn one_shared_token_is_noise_not_evidence() {
        // "play" overlapping "play something else" must not put that action on
        // the ballot — ranking is global, so weak evidence is everyone's problem.
        let action = indexed("spotify", "next", "media", &["play something else"]);
        assert!(score_action("play radiohead", &action).is_none());
    }

    #[test]
    fn a_template_captures_the_span_behind_its_placeholder() {
        let play = with_params(
            "spotify",
            "play_artist",
            "media",
            &["play {artist}", "put on some {artist}"],
            &["artist"],
        );
        let hit = score_action("put on some Arctic Monkeys", &play).unwrap();
        assert_eq!(hit.kind, MatchKind::Template);
        assert_eq!(
            hit.spans.get("artist").map(String::as_str),
            Some("arctic monkeys")
        );
    }

    #[test]
    fn a_span_between_two_literals_is_bounded_by_both() {
        let tell = with_params(
            "slack",
            "send_dm",
            "messaging",
            &["tell {who} that {message}"],
            &["who", "message"],
        );
        let hit = score_action("tell Jack that I am running late", &tell).unwrap();
        assert_eq!(hit.spans.get("who").map(String::as_str), Some("jack"));
        assert_eq!(
            hit.spans.get("message").map(String::as_str),
            Some("i am running late")
        );
    }

    #[test]
    fn literals_must_appear_in_order() {
        let tell = with_params(
            "slack",
            "send_dm",
            "messaging",
            &["tell {who} that {message}"],
            &["who", "message"],
        );
        // "that" before "tell" is a different sentence, not this template.
        assert!(score_action("that is what I would tell Jack", &tell).is_none());
    }

    #[test]
    fn a_literal_never_matches_inside_a_word() {
        // "on" inside "song" would otherwise split the utterance in the wrong
        // place and hand the extension half a word.
        let play = with_params(
            "spotify",
            "play_on",
            "media",
            &["play {track} on repeat"],
            &["track"],
        );
        assert!(score_action("play this song", &play).is_none());
    }

    #[test]
    fn an_empty_span_is_left_unfilled_rather_than_invented() {
        let play = with_params(
            "spotify",
            "play_artist",
            "media",
            &["play {artist}"],
            &["artist"],
        );
        // Nothing followed "play"; a required parameter with no span is the
        // chooser's business, not something to guess here.
        let hit = score_action("play", &play);
        assert!(hit.is_none() || !hit.unwrap().spans.contains_key("artist"));
    }

    #[test]
    fn a_score_does_not_depend_on_how_long_the_span_is() {
        // The bug this replaced: dividing matched literals by the QUERY length
        // made a long message score lower than a short one for no reason except
        // its length — and since calibration examples use a one-word filler,
        // real requests scored below the bar they were measured against and got
        // refused outright.
        let tell = with_params(
            "slack",
            "send_dm",
            "messaging",
            &["tell {who} that {message}"],
            &["who", "message"],
        );
        let short = score_action("tell Jack that hi", &tell).unwrap();
        let long = score_action(
            "tell Jack that I am running about twenty minutes late for the review",
            &tell,
        )
        .unwrap();
        assert_eq!(short.score, long.score);
    }

    #[test]
    fn a_more_specific_template_outscores_a_generic_one() {
        // "play my {x} playlist" confirms three literal tokens; "play {x}"
        // confirms one. The gap is what lets the decision layer act on the
        // first without asking about the second.
        let playlist = with_params(
            "spotify",
            "play_playlist",
            "media",
            &["play my {playlist} playlist"],
            &["playlist"],
        );
        let artist = with_params(
            "spotify",
            "play_artist",
            "media",
            &["play {artist}"],
            &["artist"],
        );
        let said = "play my gym playlist";
        let specific = score_action(said, &playlist).unwrap().score;
        let generic = score_action(said, &artist).unwrap().score;
        assert!(
            specific - generic >= 0.15,
            "the gap has to stay wider than the calibrated slack, or the decision \
             layer goes back to asking about every playlist ({specific} vs {generic})"
        );
    }

    #[test]
    fn a_command_survives_one_mangled_character() {
        // ASR substitutes rather than omits, and a single bad character used to
        // throw the whole utterance away.
        let next = indexed("spotify", "next", "media", &["skip this", "next song"]);
        assert!(score_action("skit this", &next).is_some());
        assert!(score_action("next song", &next).is_some());
    }

    #[test]
    fn short_words_get_no_spelling_tolerance() {
        // One edit turns "on" into "in" and "to" into "do" — different words,
        // not misheard ones. Tolerance there would be a false-execution source.
        assert!(!same_word("on", "in"));
        assert!(!same_word("to", "do"));
        assert!(same_word("skip", "skit"));
        assert!(same_word("playlist", "playlst"));
        // Two edits is a different word even when it is long.
        assert!(!same_word("playlist", "plarlsst"));
    }

    #[test]
    fn a_template_that_starts_late_scores_below_one_that_starts_clean() {
        // "I was going to tell Jack that…" is talking about the command, not
        // issuing it.
        let tell = with_params(
            "slack",
            "send_dm",
            "messaging",
            &["tell {who} that {message}"],
            &["who", "message"],
        );
        let clean = score_action("tell Jack that hi", &tell).unwrap();
        let late = score_action("I was going to tell Jack that hi", &tell).unwrap();
        assert!(late.score < clean.score);
    }

    #[test]
    fn ranking_is_stable_and_independent_of_registry_order() {
        let a = indexed("aaa", "next", "media", &["next song"]);
        let b = indexed("bbb", "next", "media", &["next song"]);
        let forwards = rank("next song", &[a.clone(), b.clone()]);
        let backwards = rank("next song", &[b, a]);
        let names: Vec<String> = forwards.iter().map(|c| c.extension_id.clone()).collect();
        let reversed: Vec<String> = backwards.iter().map(|c| c.extension_id.clone()).collect();
        assert_eq!(names, reversed, "install order must not decide the ballot");
    }

    #[test]
    fn two_providers_of_one_request_land_in_one_class() {
        // D1: without this the top-two margin is permanently zero and every
        // media command falls to the chooser, so the provider ladder — which
        // exists for exactly this case — never runs.
        let actions = [
            indexed("spotify", "next", "media", &["skip this", "next song"]),
            indexed("apple", "next_track", "media", &["next song", "skip this"]),
        ];
        let classes = equivalence_classes(&actions);
        assert_eq!(classes.len(), 1);
        assert_eq!(
            classes.class_of("spotify", "next"),
            classes.class_of("apple", "next_track")
        );
    }

    #[test]
    fn the_same_words_in_different_domains_stay_apart() {
        // "next slide" and "next track" are precisely what must NOT be merged.
        let actions = [
            indexed("spotify", "next", "media", &["next one", "move on"]),
            indexed("deck", "next_slide", "system", &["next one", "move on"]),
        ];
        let classes = equivalence_classes(&actions);
        assert_eq!(classes.len(), 2);
        assert_ne!(
            classes.class_of("spotify", "next"),
            classes.class_of("deck", "next_slide")
        );
    }

    #[test]
    fn a_generic_phrase_does_not_drag_a_specific_one_into_its_class() {
        // Found by the golden set: "play" (Apple's play_artist) overlapped
        // "play my playlist" (Spotify's play_playlist) at exactly the set
        // threshold, linking two unrelated actions — and the union then pulled
        // Spotify's own play_artist in behind them. "Play my gym playlist"
        // became a provider chooser.
        let actions = [
            with_params(
                "spotify",
                "play_playlist",
                "media",
                &[
                    "play my {playlist} playlist",
                    "start my {playlist} playlist",
                ],
                &["playlist"],
            ),
            with_params(
                "spotify",
                "play_artist",
                "media",
                &["play {artist}"],
                &["artist"],
            ),
            with_params(
                "apple",
                "play_artist",
                "media",
                &["play {artist}"],
                &["artist"],
            ),
        ];
        let classes = equivalence_classes(&actions);
        assert_eq!(
            classes.class_of("spotify", "play_artist"),
            classes.class_of("apple", "play_artist"),
            "the same request from two providers is still one class"
        );
        assert_ne!(
            classes.class_of("spotify", "play_playlist"),
            classes.class_of("spotify", "play_artist"),
            "a playlist request is not an artist request"
        );
    }

    #[test]
    fn a_merge_never_collapses_one_extension_through_a_third_party() {
        // Grouping is transitive, so A(ext1)≡B(ext2) and B(ext2)≡C(ext1) would
        // put two of ext1's own actions in one class without them ever being
        // compared — silently making one of them unreachable.
        let actions = [
            indexed("spotify", "a", "media", &["skip this", "next song"]),
            indexed(
                "apple",
                "next",
                "media",
                &["skip this", "next song", "go back"],
            ),
            indexed("spotify", "b", "media", &["go back", "next song"]),
        ];
        let classes = equivalence_classes(&actions);
        assert_ne!(
            classes.class_of("spotify", "a"),
            classes.class_of("spotify", "b"),
            "one author's actions must never be collapsed, however they are linked"
        );
    }

    #[test]
    fn one_extension_never_merges_with_itself() {
        // An author's own vocabulary is theirs to keep distinct; merging their
        // two actions would silently make one unreachable.
        let actions = [
            indexed("spotify", "next", "media", &["skip this"]),
            indexed("spotify", "next_album", "media", &["skip this"]),
        ];
        let classes = equivalence_classes(&actions);
        assert_eq!(classes.len(), 2);
    }

    #[test]
    fn doctor_separates_a_real_clash_from_expected_provider_overlap() {
        let actions = [
            indexed("spotify", "next", "media", &["skip this"]),
            indexed("apple", "next", "media", &["skip this"]),
            indexed("deck", "next_slide", "system", &["skip this"]),
        ];
        let found = collisions(&actions);
        let provider = found
            .iter()
            .find(|c| c.left == "apple:next" || c.right == "apple:next")
            .expect("the two media actions collide");
        assert!(
            provider.is_provider_overlap,
            "two providers of one request is the expected shape, not an author mistake"
        );
        let cross = found
            .iter()
            .find(|c| c.left.starts_with("deck") || c.right.starts_with("deck"))
            .expect("the deck action collides across domains");
        assert!(
            !cross.is_provider_overlap,
            "a cross-domain clash is a real problem the author has to fix"
        );
    }

    #[test]
    fn an_empty_query_scores_nothing() {
        let next = indexed("spotify", "next", "media", &["skip this"]);
        assert!(score_action("", &next).is_none());
        assert!(score_action("...", &next).is_none());
    }
}
