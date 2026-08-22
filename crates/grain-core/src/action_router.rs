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

/// How many candidate actions one query may gather before the walk stops.
///
/// Generous relative to any real decision — the prediction set is at most a
/// handful — and small enough that calibration over a large installed set stays
/// in milliseconds rather than seconds.
const MAX_CANDIDATES: usize = 64;

/// Keys a token is filed under, so an inverted-index lookup can find every
/// token within [`same_word`]'s tolerance without scanning.
///
/// The **deletion neighbourhood** (SymSpell's construction): file a token under
/// itself and under each of its single-character deletions. Two strings within
/// one edit always share a key, so one hash lookup per key finds every fuzzy
/// match — no scan, and no enumerating 26 substitutions per position.
///
/// Tokens shorter than four characters get no tolerance in `same_word`, so they
/// are filed under themselves alone. The two functions must agree, or the index
/// prunes away candidates the matcher would have accepted.
fn fuzzy_keys(token: &str) -> Vec<String> {
    let mut keys = vec![token.to_string()];
    if token.len() >= 4 && token.is_ascii() {
        for skip in 0..token.len() {
            let mut variant = String::with_capacity(token.len() - 1);
            variant.push_str(&token[..skip]);
            variant.push_str(&token[skip + 1..]);
            keys.push(variant);
        }
    }
    keys
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

/// The installed set, plus the corpus statistics scoring depends on.
///
/// Built once per index rebuild. Nothing in here is recomputed on the felt path.
#[derive(Clone, Debug, Default)]
pub struct ActionIndex {
    actions: Vec<IndexedAction>,
    /// How much evidence one literal token carries, by how rare it is across
    /// every declared utterance in the installed set.
    idf: HashMap<String, f32>,
    /// Fuzzy key → actions whose literals contain a token reachable from it.
    ///
    /// Scoring every action against every utterance is fine on the felt path
    /// (hundreds of cheap matches, ~1 ms) and catastrophic in calibration, which
    /// multiplies it by the sample count: 300 actions took **6.5 seconds in
    /// release**, on a rebuild that runs whenever a switch is flipped. This
    /// prunes to the actions that share a literal token with what was said,
    /// which is a superset of what can possibly match — `find_tokens` needs at
    /// least one confirmed token — so it changes no result.
    postings: HashMap<String, Vec<u32>>,
}

impl ActionIndex {
    pub fn build(actions: Vec<IndexedAction>) -> Self {
        // Each declared utterance is a document; a literal token's document
        // frequency is how many of them contain it.
        let mut document_frequency: HashMap<String, usize> = HashMap::new();
        let mut documents = 0usize;
        let mut postings: HashMap<String, Vec<u32>> = HashMap::new();
        for (position, action) in actions.iter().enumerate() {
            for template in &action.templates {
                documents += 1;
                let mut seen = std::collections::HashSet::new();
                for part in template {
                    let UtterancePart::Literal(literal) = part else {
                        continue;
                    };
                    for token in tokens(&normalise(literal)) {
                        if seen.insert(token.to_string()) {
                            *document_frequency.entry(token.to_string()).or_insert(0) += 1;
                        }
                        for key in fuzzy_keys(token) {
                            postings.entry(key).or_default().push(position as u32);
                        }
                    }
                }
            }
        }
        for list in postings.values_mut() {
            list.sort_unstable();
            list.dedup();
        }
        let n = documents as f32;
        let idf = document_frequency
            .into_iter()
            .map(|(token, df)| {
                // BM25's IDF. The shape matters more than the constant: rarity
                // decays logarithmically, because a token in half as many
                // utterances is not twice as diagnostic.
                let df = df as f32;
                let value = ((n - df + 0.5) / (df + 0.5) + 1.0).ln();
                (token, value.max(0.0))
            })
            .collect();
        ActionIndex {
            actions,
            idf,
            postings,
        }
    }

    pub fn actions(&self) -> &[IndexedAction] {
        &self.actions
    }

    pub fn is_empty(&self) -> bool {
        self.actions.is_empty()
    }

    /// Evidence carried by a literal token. Unknown tokens cannot occur (the
    /// index built the table from these very templates), but a neutral default
    /// keeps this total.
    fn evidence(&self, token: &str) -> f32 {
        self.idf.get(token).copied().unwrap_or(1.0)
    }

    /// Actions that share at least one literal token with the query, allowing
    /// the same single-edit tolerance the matcher uses.
    ///
    /// Query tokens are consulted **rarest first**, and the walk stops once
    /// enough candidates are gathered. That bound matters: postings alone prune
    /// nothing when a token is shared by most of the installed set, which is not
    /// hypothetical — "open", "play" and "next" will each be declared by dozens
    /// of extensions.
    ///
    /// Dropping the common tokens' postings is an approximation, and a safe one
    /// in the direction that counts. An action reachable *only* through a token
    /// that half the corpus declares has almost no IDF evidence behind it, so it
    /// scores near the bottom and could not have cleared the calibrated floor.
    /// The candidates given up are the ones that were never going to win.
    fn candidates(&self, query: &str) -> Vec<u32> {
        let mut query_tokens = tokens(query);
        // Rarest first: the discriminative tokens are the ones worth spending
        // the budget on, which is the same argument IDF makes about evidence.
        query_tokens.sort_by(|a, b| {
            self.idf
                .get(*b)
                .unwrap_or(&f32::MAX)
                .total_cmp(self.idf.get(*a).unwrap_or(&f32::MAX))
        });
        let mut out = Vec::new();
        for token in query_tokens {
            for key in fuzzy_keys(token) {
                if let Some(list) = self.postings.get(&key) {
                    out.extend_from_slice(list);
                }
            }
            if out.len() >= MAX_CANDIDATES {
                break;
            }
        }
        out.sort_unstable();
        out.dedup();
        out
    }

    /// Score every plausible action, best first.
    pub fn rank(&self, spoken: &str) -> Vec<Candidate> {
        let query = normalise(spoken);
        let mut out: Vec<Candidate> = self
            .candidates(&query)
            .into_iter()
            .filter_map(|position| {
                let action = self.actions.get(position as usize)?;
                self.score_action(spoken, action)
            })
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

    /// Score one utterance against one action, returning its best template.
    pub fn score_action(&self, spoken: &str, action: &IndexedAction) -> Option<Candidate> {
        score_action_with(self, spoken, action)
    }
}

fn score_action_with(
    index: &ActionIndex,
    spoken: &str,
    action: &IndexedAction,
) -> Option<Candidate> {
    let query = normalise(spoken);
    if query.is_empty() {
        return None;
    }
    let query_tokens = tokens(&query);
    let mut best: Option<(f32, MatchKind, HashMap<String, String>)> = None;

    for template in &action.templates {
        let scored = score_template(index, &query, &query_tokens, template);
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
        score,
        kind,
        spans,
    })
}

/// Match one template. Literals must appear **in order**; whatever falls between
/// two literals is the span for the placeholder between them.
fn score_template(
    index: &ActionIndex,
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

    // With placeholders: walk the template over the query's TOKENS, left to
    // right.
    //
    // Tokens rather than byte offsets, and that is not a tidiness choice. The
    // first version searched for each literal as a substring, so the spelling
    // tolerance below never applied to templates at all — only to whole-phrase
    // matches. One mangled character in "playlist" made
    // `play my {x} playlist` fail outright while `play {artist}` still matched,
    // which put an enormous tail in the calibration margins and forced a slack
    // so wide it swallowed every real distinction. The matcher and the
    // corruption model have to agree about what a mishearing is.
    let mut spans: HashMap<String, String> = HashMap::new();
    let mut cursor = 0usize;
    let mut matched_literal_tokens = 0usize;
    let mut evidence = 0.0f32;
    let mut pending: Option<&str> = None;
    let starts_with_literal = matches!(template.first(), Some(UtterancePart::Literal(_)));
    let mut leading_junk = false;

    for part in template {
        match part {
            UtterancePart::Literal(literal) => {
                let normalised = normalise(literal);
                let needle = tokens(&normalised);
                if needle.is_empty() {
                    continue;
                }
                let found = find_tokens(query_tokens, cursor, &needle)?;
                // A template that begins with a literal expects the utterance to
                // begin there too. Anything in front is unexplained by both the
                // literals and the spans, which is weaker evidence than a clean
                // match — "I was going to tell Jack that…" is not the command.
                if starts_with_literal && matched_literal_tokens == 0 && found.start > 0 {
                    leading_junk = true;
                }
                if let Some(name) = pending.take() {
                    // A placeholder with nothing in front of the next literal
                    // has no value; a required one sends this to the chooser
                    // rather than being invented here.
                    let span = query_tokens[cursor..found.start].join(" ");
                    if !span.is_empty() {
                        spans.insert(name.to_string(), span);
                    }
                }
                for i in &found.matched {
                    matched_literal_tokens += 1;
                    // Evidence comes from the DECLARED token, not the possibly
                    // misheard one: what the author wrote is what the corpus
                    // statistics were built from. A token that never arrived
                    // contributes nothing, so a match missing a word is worth
                    // strictly less than one that is complete.
                    evidence += index.evidence(needle[*i]);
                }
                cursor = found.end;
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
        let span = query_tokens[cursor..].join(" ");
        if !span.is_empty() {
            spans.insert(name.to_string(), span);
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
    // Score on the template's own specificity instead — but specificity is
    // **rarity, not count**. Counting tokens credits three generic words
    // ("play the one") as heavily as one diagnostic one ("playlist"), which is
    // backwards. BM25's insight applies directly: a token in half as many
    // utterances is not twice as diagnostic, so evidence is IDF-weighted and
    // saturates logarithmically rather than climbing linearly to a cliff at
    // some hand-picked count.
    let specificity = evidence / (evidence + EVIDENCE_KNEE);
    let mut score = TEMPLATE_FLOOR + (TEMPLATE_CEILING - TEMPLATE_FLOOR) * specificity;
    if leading_junk {
        score -= LEADING_JUNK_PENALTY;
    }
    let _ = query_tokens;
    Some((score, MatchKind::Template, spans))
}

/// Where accumulated IDF evidence is worth half of the available range. Sets the
/// knee of the saturation curve, not a cutoff — there is no count at which a
/// template abruptly becomes "specific".
const EVIDENCE_KNEE: f32 = 3.0;
/// A template whose literals carry almost no evidence ("play {artist}", where
/// "play" appears in half the installed vocabulary) — a real match, weak
/// evidence.
///
/// Lower than it was when specificity counted tokens. IDF saturation is flat in
/// the region these templates live in, so the same difference in specificity
/// produces a smaller difference in score; widening the range restores the
/// separation the decision layer needs to act rather than ask.
const TEMPLATE_FLOOR: f32 = 0.45;
/// A fully specific template. Below 1.0 on purpose: an exact declared phrase is
/// always the stronger claim.
const TEMPLATE_CEILING: f32 = 0.95;
/// Charged when a template that begins with a literal did not begin the
/// utterance.
const LEADING_JUNK_PENALTY: f32 = 0.15;

/// Where a literal was found, and which of its tokens actually appeared.
struct LiteralMatch {
    /// Token index in the query where the literal starts.
    start: usize,
    /// Token index just past where it ends.
    end: usize,
    /// Indices into the needle that were confirmed. A token the channel
    /// swallowed is absent here, so it contributes no evidence — the match
    /// survives, and it is honestly worth less.
    matched: Vec<usize>,
}

/// Find `needle`'s tokens in `haystack` at or after `from`.
///
/// Token-wise by construction, so "on" can never match inside "song". Two
/// tolerances, and both exist because the corruption model in
/// `action_decision::corrupt` says the channel does exactly these things:
///
/// - each token gets the spelling tolerance of [`same_word`] (substitution);
/// - **at most one** needle token may be missing entirely (deletion of an
///   unstressed function word — "send Jack a message" arriving as "send Jack
///   message").
///
/// Matcher and corruption model have to agree. When they did not, calibration
/// saw the true action score zero on inputs the model claimed were routine, and
/// the resulting slack was wide enough to swallow every real distinction.
fn find_tokens(haystack: &[&str], from: usize, needle: &[&str]) -> Option<LiteralMatch> {
    if needle.is_empty() || from > haystack.len() {
        return None;
    }
    let mut best: Option<LiteralMatch> = None;
    for start in from..haystack.len() {
        let mut matched = Vec::with_capacity(needle.len());
        let mut skipped = false;
        let mut j = start;
        let mut ok = true;
        for (i, want) in needle.iter().enumerate() {
            if j < haystack.len() && same_word(haystack[j], want) {
                matched.push(i);
                j += 1;
            } else if !skipped {
                skipped = true;
            } else {
                ok = false;
                break;
            }
        }
        // A literal that matched nothing has not been found — otherwise a
        // one-token literal would "match" by skipping itself, and every
        // template would fire on every utterance.
        if !ok || matched.is_empty() {
            continue;
        }
        let candidate = LiteralMatch {
            start,
            end: j,
            matched,
        };
        // Prefer the earliest position, and at a given position the alignment
        // that confirmed the most tokens.
        if best
            .as_ref()
            .is_none_or(|b| candidate.matched.len() > b.matched.len() && candidate.start == b.start)
        {
            let complete = candidate.matched.len() == needle.len();
            best = Some(candidate);
            if complete {
                break;
            }
        }
    }
    best
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

/// How closely two individual phrases must match to count as the same phrasing.
/// Deliberately strict: a short generic phrase overlapping a longer specific one
/// is not evidence that two declarations mean the same thing.
const PHRASE_MATCH_THRESHOLD: f32 = 0.8;

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

// ── Author diagnostics (`grain-ext doctor`) ─────────────────────────────────

/// Two of one extension's own declarations whose language overlaps enough that
/// its internal matching will struggle to tell them apart.
#[derive(Clone, Debug, PartialEq)]
pub struct Collision {
    pub left: String,
    pub right: String,
    pub shared: String,
}

/// Find one extension's declarations that will fight each other at match time.
///
/// Scoped to a single extension on purpose. Under V1 Grain ranks *extensions*,
/// never their commands, so Spotify's `next` and Apple Music's `next` are not
/// rivals — they are never compared, and warning about them would be noise the
/// author cannot act on. Two of the *same* author's phrasings overlapping is
/// still a real defect: it makes one of them unreachable.
pub fn collisions(actions: &[IndexedAction]) -> Vec<Collision> {
    let phrases: Vec<Vec<String>> = actions.iter().map(literal_phrases).collect();
    let mut out = Vec::new();
    for a in 0..actions.len() {
        for b in (a + 1)..actions.len() {
            if actions[a].extension_id != actions[b].extension_id {
                continue;
            }
            let shared = phrases[a].iter().find(|left| {
                phrases[b].iter().any(|right| {
                    left == &right || token_overlap(&tokens(left), right) >= PHRASE_MATCH_THRESHOLD
                })
            });
            if let Some(shared) = shared {
                out.push(Collision {
                    left: actions[a].qualified(),
                    right: actions[b].qualified(),
                    shared: shared.clone(),
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

    fn decl(id: &str, utterances: &[&str], params: &[&str]) -> ActionDecl {
        ActionDecl {
            id: id.into(),
            title: id.into(),
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

    fn indexed(ext: &str, id: &str, utterances: &[&str]) -> IndexedAction {
        IndexedAction::from_decl(ext, &decl(id, utterances, &[]))
    }

    /// Score one action in isolation. Corpus statistics come from that action's
    /// own utterances, which is the right baseline for tests about matching
    /// mechanics; tests about *competition* build a shared index instead.
    fn score_action(said: &str, action: &IndexedAction) -> Option<Candidate> {
        ActionIndex::build(vec![action.clone()]).score_action(said, action)
    }

    fn rank(said: &str, actions: &[IndexedAction]) -> Vec<Candidate> {
        ActionIndex::build(actions.to_vec()).rank(said)
    }

    fn with_params(ext: &str, id: &str, utterances: &[&str], params: &[&str]) -> IndexedAction {
        IndexedAction::from_decl(ext, &decl(id, utterances, params))
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
        let next = indexed("spotify", "next", &["skip this", "next song"]);
        let hit = score_action("Skip this.", &next).unwrap();
        assert_eq!(hit.kind, MatchKind::Exact);
        assert_eq!(hit.score, 1.0);
    }

    #[test]
    fn one_shared_token_is_noise_not_evidence() {
        // "play" overlapping "play something else" must not put that action on
        // the ballot — ranking is global, so weak evidence is everyone's problem.
        let action = indexed("spotify", "next", &["play something else"]);
        assert!(score_action("play radiohead", &action).is_none());
    }

    #[test]
    fn a_template_captures_the_span_behind_its_placeholder() {
        let play = with_params(
            "spotify",
            "play_artist",
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
            &["play {track} on repeat"],
            &["track"],
        );
        assert!(score_action("play this song", &play).is_none());
    }

    #[test]
    fn an_empty_span_is_left_unfilled_rather_than_invented() {
        let play = with_params("spotify", "play_artist", &["play {artist}"], &["artist"]);
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
            &["play my {playlist} playlist"],
            &["playlist"],
        );
        let artist = with_params("spotify", "play_artist", &["play {artist}"], &["artist"]);
        // One shared index: specificity is measured in IDF over the whole
        // installed set, so scoring these separately would compare two different
        // corpora and prove nothing.
        let index = ActionIndex::build(vec![playlist, artist]);
        let ranked = index.rank("play my gym playlist");
        assert_eq!(ranked[0].action_id, "play_playlist");
        let gap = ranked[0].score - ranked[1].score;
        // Only two utterances in this index, so IDF is at its most compressed —
        // a realistic installed set separates these further. The bar is set for
        // the worst case on purpose.
        assert!(
            gap >= 0.10,
            "the gap has to stay wider than the calibrated slack, or the decision \
             layer goes back to asking about every playlist (gap {gap})"
        );
    }

    #[test]
    fn a_command_survives_one_mangled_character() {
        // ASR substitutes rather than omits, and a single bad character used to
        // throw the whole utterance away.
        let next = indexed("spotify", "next", &["skip this", "next song"]);
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
            &["tell {who} that {message}"],
            &["who", "message"],
        );
        let clean = score_action("tell Jack that hi", &tell).unwrap();
        let late = score_action("I was going to tell Jack that hi", &tell).unwrap();
        assert!(late.score < clean.score);
    }

    #[test]
    fn ranking_is_stable_and_independent_of_registry_order() {
        let a = indexed("aaa", "next", &["next song"]);
        let b = indexed("bbb", "next", &["next song"]);
        let forwards = rank("next song", &[a.clone(), b.clone()]);
        let backwards = rank("next song", &[b, a]);
        let names: Vec<String> = forwards.iter().map(|c| c.extension_id.clone()).collect();
        let reversed: Vec<String> = backwards.iter().map(|c| c.extension_id.clone()).collect();
        assert_eq!(names, reversed, "install order must not decide the ballot");
    }

    #[test]
    fn doctor_reports_an_author_against_themselves_and_no_one_else() {
        // Two providers of one request used to be the interesting case. Under
        // V1 they are never compared — Grain ranks extensions, and each ranks
        // its own commands — so the only actionable clash is an author's own.
        let actions = [
            indexed("spotify", "next", &["skip this"]),
            indexed("spotify", "next_album", &["skip this"]),
            indexed("apple", "next", &["skip this"]),
        ];
        let found = collisions(&actions);
        assert_eq!(found.len(), 1, "only the intra-extension clash is reported");
        assert_eq!(found[0].left, "spotify:next");
        assert_eq!(found[0].right, "spotify:next_album");
    }

    #[test]
    fn an_empty_query_scores_nothing() {
        let next = indexed("spotify", "next", &["skip this"]);
        assert!(score_action("", &next).is_none());
        assert!(score_action("...", &next).is_none());
    }
}
