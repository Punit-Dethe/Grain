//! [GRAIN] What to do with a ranking (`docs/Action Routing/PLAN.md` §4.3–§4.5).
//!
//! The router says how well things matched. This says whether to run one, ask,
//! hand it to the Agent, or say nothing can do it — and it is the part that
//! decides whether the feature can be trusted at all.
//!
//! # Why conformal prediction and not a threshold
//!
//! The plan's first cut had a floor and a margin "computed from cohesion and
//! separation". That is better than a magic constant and still unprincipled:
//! there is no statement of the form *"this misfires at most X% of the time"*,
//! which is the only claim that actually matters for a key with side effects.
//!
//! **Split conformal prediction** gives exactly that statement, distribution
//! free. Calibrate on labelled examples, take the α-quantile of the
//! nonconformity scores, and the resulting prediction set contains the true
//! class with probability ≥ 1−α — so what is guaranteed is *"the right answer is
//! in the list"*, not *"the top one is right"*. Then the set's SIZE is the
//! decision, and that is where the safety comes from: we only act when the
//! guaranteed-to-contain-it list has exactly one member.
//!
//! | set | meaning | outcome |
//! |---|---|---|
//! | 0 | nothing cleared the bar | nothing installed can do that |
//! | 1 | one request, confidently | execute (provider ladder permitting) |
//! | 2–4 | a small, honest ambiguity | ask — and the true answer is in the list |
//! | >4 | no reasonable question to ask | escalate to the Agent |
//!
//! That mapping is CICC (den Hengst et al., *Conformal Intent Classification
//! and Clarification*, NAACL Findings 2024), which is the same shape arrived at
//! independently in PLAN §4.3 — with the important addition that the
//! clarification list is *guaranteed* to contain the right answer at the chosen
//! confidence, so a chooser is never a dead end.
//!
//! # Where the calibration data comes from, and what that costs
//!
//! From the declared utterances, corrupted the way speech corrupts them — see
//! [`calibrate`] for the full argument and [`corrupt`] for the model itself. It
//! runs at index build, needs no user data, and never leaves the machine.
//!
//! The honest caveat: conformal's guarantee is marginal over the *calibration*
//! distribution, and a corruption model is not real speech. So the coverage
//! claim is approximate under that shift, and α is a dial with a stated meaning
//! rather than a proof. That is still categorically better than a constant
//! somebody picked, because the assumption is written down where it can be
//! argued with.
//!
//! Two quantiles come out of one calibration run, answering different questions:
//! an absolute **floor** (is anything here at all?) and a relative **slack**
//! (given the leader, who else is plausible?). The floor alone builds sets far
//! too generous to act on; the slack alone cannot reject at all, because every
//! utterance has a leader and a leader is always within zero of itself — an
//! utterance matching nothing would still produce a set of size one and execute
//! it.
//!
//! In the nonconformity taxonomy those are **Hinge** and **Margin**. Combining
//! them is unusual and deliberate: open-set rejection and set membership are
//! genuinely different questions here. APS/RAPS are the better single scores in
//! general, and both need a probability distribution over classes — which a
//! lexical matcher does not produce and should not fake.
//!
//! Because the set is the **intersection** of two conditions, the error budget
//! is split between them (Bonferroni) — see [`calibrate`].
//!
//! Per-class (Mondrian) floors are used wherever an action supplies enough
//! samples to support the requested α; the rest fall back to the pooled floor.
//! Small-n honesty matters: below a certain count there is no finite quantile,
//! and clamping instead of admitting that is how a calibrated system quietly
//! stops being calibrated.

use crate::action_router::{
    equivalence_classes, ActionIndex, Candidate, EquivalenceMap, IndexedAction,
};
use grain_sdk::manifest::ActionRisk;
use std::collections::{BTreeMap, HashMap};

/// One thing the router could do, fully resolved apart from the user's consent.
#[derive(Clone, Debug, PartialEq)]
pub struct Selection {
    pub extension_id: String,
    pub action_id: String,
    pub domain: String,
    pub title: String,
    pub risk: ActionRisk,
    /// Raw spans, unresolved. The extension matches these against its own
    /// catalogue; Grain does not know what a playlist is called.
    pub spans: BTreeMap<String, String>,
    /// Required parameters with no span. A non-empty list is never executed.
    pub missing: Vec<String>,
    pub score: f32,
}

impl Selection {
    fn from(candidate: &Candidate, action: &IndexedAction) -> Self {
        let spans: BTreeMap<String, String> = candidate
            .spans
            .iter()
            .map(|(k, v)| (k.clone(), v.clone()))
            .collect();
        let missing = action
            .required_params
            .iter()
            .filter(|name| !spans.contains_key(*name))
            .cloned()
            .collect();
        Selection {
            extension_id: candidate.extension_id.clone(),
            action_id: candidate.action_id.clone(),
            domain: candidate.domain.clone(),
            title: action.title.clone(),
            risk: action.risk,
            spans,
            missing,
            score: candidate.score,
        }
    }

    /// Whether running this needs a read-back of the resolved action first.
    ///
    /// Confidence never enters. The adversary is ASR substitution — "cancel my
    /// order" heard as "schedule my order" scores well and parses cleanly — so
    /// a high score is evidence about the transcript, never about the speech.
    pub fn needs_confirmation(&self) -> bool {
        self.risk == ActionRisk::Confirm
    }
}

/// Why nothing ran.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RefuseReason {
    /// Empty or unusable capture. Never escalated: an empty prompt is not a
    /// question for a language model.
    NothingHeard,
    /// Everything scored below its calibrated bar. The out-of-scope class that
    /// actually matters once the trigger is its own key — not "that was really
    /// dictation" but "no installed extension does this".
    NothingInstalledCanDoThat,
    /// The request needed the Agent and there is no model configured.
    NeedsAgentButNoneConfigured,
}

/// What should happen.
#[derive(Clone, Debug, PartialEq)]
pub enum Outcome {
    Execute(Selection),
    /// Ask. The list is a conformal prediction set, so at the configured
    /// confidence the intended answer is in it — a chooser is never a dead end.
    Choose {
        options: Vec<Selection>,
        reason: ChooseReason,
    },
    /// Hand a small hydrated set to the Agent: compound, or too ambiguous for an
    /// honest question.
    Escalate(Vec<Selection>),
    Refuse(RefuseReason),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ChooseReason {
    /// Several different requests are plausible.
    WhichAction,
    /// The request is decided; more than one installed extension can serve it
    /// and no rung of the provider ladder picked a winner.
    WhichProvider,
    /// A required parameter has no span.
    MissingDetail,
}

/// Largest clarification list worth showing.
///
/// CICC's finding, and common sense: past a handful there is no reasonable
/// ground for a clarification question, and a long list is a worse experience
/// than admitting the request needs reasoning.
const MAX_CLARIFY: usize = 4;

/// How many actions are hydrated into an Agent turn. Small on purpose — the
/// whole point of routing before the model is that the model sees a handful.
const MAX_ESCALATE: usize = 6;

// ── Calibration ─────────────────────────────────────────────────────────────

/// Conformal thresholds over the installed set, rebuilt whenever it changes.
#[derive(Clone, Debug)]
pub struct Calibration {
    /// Score floor per equivalence class, where enough data supported one.
    by_class: HashMap<usize, f32>,
    /// Pooled floor, used for classes too small to calibrate on their own.
    pooled: f32,
    /// How far behind the leader the intended action is still allowed to be.
    ///
    /// Two quantiles rather than one, because the two questions are different:
    /// the **floor** is absolute and answers *"is anything here at all?"* — the
    /// out-of-scope test. The **slack** is relative and answers *"who else is
    /// plausible given the leader?"* — the set-membership test.
    ///
    /// An absolute floor alone builds sets that are far too generous: with a
    /// 0.95 match and a 0.65 match both above the bar, "play my gym playlist"
    /// asked whether the user meant an artist. A raw score is a poor
    /// nonconformity measure precisely because it ignores the field.
    ///
    /// Pooled rather than per-class on purpose: a margin is a statement about
    /// how this installed set separates, which is a property of the whole index,
    /// not of one action.
    slack: f32,
    pub alpha: f32,
    pub samples: usize,
}

impl Calibration {
    /// What to use before [`calibrate`] has finished.
    ///
    /// Calibrating a large installed set takes seconds, and it runs whenever the
    /// extension set changes — which is a switch being flipped, where a stall is
    /// very visible. So the host builds the index synchronously, routes with
    /// this, and swaps in the real thing when the background pass lands.
    ///
    /// Safe in the direction that matters: a high floor and no slack means only
    /// a near-exact match executes. The degradation is "asks more often for a
    /// moment", never "acts on a guess".
    pub fn conservative() -> Self {
        Calibration {
            by_class: HashMap::new(),
            pooled: UNCALIBRATED_FLOOR,
            slack: 0.0,
            alpha: DEFAULT_ALPHA,
            samples: 0,
        }
    }

    /// Whether this came from data. `false` means [`Calibration::conservative`]
    /// is still in place.
    pub fn is_calibrated(&self) -> bool {
        self.samples > 0
    }

    /// The pooled score floor. Exposed so the operating point can be reported
    /// rather than inferred — PLAN §12 asks for it per phase.
    pub fn pooled_floor(&self) -> f32 {
        self.pooled
    }

    /// How far behind the leader a candidate may sit and still be in the set.
    pub fn slack(&self) -> f32 {
        self.slack
    }

    fn floor(&self, class: Option<usize>) -> f32 {
        class
            .and_then(|c| self.by_class.get(&c).copied())
            .unwrap_or(self.pooled)
    }
}

/// The floor used when there is nothing to calibrate on at all — an installed
/// set whose actions declare too little. Conservative: with no evidence about
/// how this set behaves, only a near-exact match should run.
const UNCALIBRATED_FLOOR: f32 = 0.95;

/// Miscoverage budget: how often the intended action may be **missing from the
/// prediction set**. At 0.05, it is present ~95% of the time.
///
/// Note the direction, because it is easy to get backwards and this codebase
/// did: a *smaller* α means *higher* coverage, which means a *lower* score
/// floor and *larger* sets. So tightening α produces more clarification
/// questions, not fewer — and fewer wrong executions, because the wrong ones are
/// mostly cases where the right answer fell out of the set and a rival was left
/// alone in it.
///
/// α is therefore the honest knob for "how often may we lose the right answer",
/// and the execute/ask boundary is the separate knob for "how sure before we
/// act". Nothing else in this file should be tuned instead of these two.
pub const DEFAULT_ALPHA: f32 = 0.05;

/// Calibrate on the declared utterances under a **speech-corruption model**.
///
/// # Why not leave-one-out
///
/// The obvious construction — hold out one declared utterance, score it against
/// the rest — was tried first and is wrong for this tier. It measures
/// generalisation *across phrasings*, which Tier L deliberately does not have:
/// that is precisely what the semantic leg is for. Held-out synonyms score ~0,
/// the quantile collapses, and either everything routes or nothing does. In the
/// first run, "tell Jack that I am running late" was refused outright.
///
/// # What is measured instead
///
/// The gap that actually exists at Tier L is between the phrase an author
/// *declared* and the transcript that arrives from a microphone. So each
/// declared utterance is corrupted the way speech is — a filler word, a dropped
/// token, a one-character substitution of the kind the matcher tolerates — and
/// scored against the real index. The (1−α) quantile of those scores is the
/// floor.
///
/// That makes the guarantee an honest one about a stated distribution: *given
/// speech that differs from the declaration the way speech does, the intended
/// action is in the prediction set ~(1−α) of the time.* Real speech is not
/// exactly this distribution, so the coverage claim is approximate — but the
/// corruption model is written down and can be argued with, which a hand-picked
/// constant cannot.
pub fn calibrate(index: &ActionIndex, alpha: f32) -> Calibration {
    let classes = equivalence_classes(index.actions());
    let mut per_class: HashMap<usize, Vec<f32>> = HashMap::new();
    let mut all = Vec::new();

    let mut margins = Vec::new();

    // Bounded, because the naive cost is a product of four things that all grow
    // with the installed set: every action's every utterance, times every
    // corruption, ranked against every action's every template. At a
    // pessimistic twenty extensions that is ~10^8 template matches on a switch
    // toggle — and `refresh_index` runs on enable, disable, grant and import,
    // where a multi-second hitch is very visible.
    //
    // Sampling is the right answer rather than a compromise: a conformal
    // quantile needs *enough* points, not all of them, and the budget below
    // supports α well past anything sane. The share is per action so a large
    // installed set thins every action equally instead of starving the ones
    // that happen to sort last.
    let action_count = index.actions().len().max(1);
    let budget_per_action = (MAX_CALIBRATION_SAMPLES / action_count).max(MIN_SAMPLES_PER_ACTION);

    'actions: for action in index.actions() {
        let class = classes.class_of(&action.extension_id, &action.action_id);
        let mut taken = 0usize;
        for template in &action.templates {
            let Some(clean) = render_template(template) else {
                continue;
            };
            for spoken in corrupt(&clean) {
                if taken >= budget_per_action {
                    break;
                }
                // The per-action share is a floor for fairness, not a licence to
                // exceed the total. On a very large installed set the tail of
                // actions calibrates on the pooled floor instead of its own,
                // which is a documented degradation rather than a stall.
                if all.len() >= MAX_CALIBRATION_SAMPLES {
                    break 'actions;
                }
                taken += 1;
                let ranked = index.rank(&spoken);
                let score = ranked
                    .iter()
                    .find(|c| classes.class_of(&c.extension_id, &c.action_id) == class)
                    .map(|c| c.score)
                    .unwrap_or(0.0);
                let leader = ranked.first().map(|c| c.score).unwrap_or(0.0);
                all.push(score);
                // How far behind the leader the RIGHT answer sat. Usually zero —
                // the tail is what the slack has to cover.
                margins.push((leader - score).max(0.0));
                if let Some(class) = class {
                    per_class.entry(class).or_default().push(score);
                }
            }
        }
    }

    // **Split the error budget.** The prediction set is the intersection of two
    // conditions, so the true class is lost if EITHER fails, and by the union
    // bound the miscoverage is at most their sum. Calibrating both at alpha
    // would claim 1-alpha coverage while delivering 1-2*alpha: a wrong claim
    // rather than a wrong behaviour, which is the more dangerous kind.
    //
    // Bonferroni: alpha/2 to each. Known to be conservative — it over-covers,
    // producing slightly larger sets than strictly necessary, because it splits
    // the budget evenly regardless of which condition is doing the work. That is
    // the right default with no evidence about their relative difficulty, and an
    // unequal split is a later refinement that needs data to justify.
    let half = alpha / 2.0;
    let pooled = conformal_floor(&mut all, half).unwrap_or(UNCALIBRATED_FLOOR);
    let slack = conformal_slack(&mut margins, half).unwrap_or(UNCALIBRATED_SLACK);
    let by_class = per_class
        .into_iter()
        .filter_map(|(class, mut scores)| {
            conformal_floor(&mut scores, half).map(|floor| (class, floor))
        })
        .collect();

    Calibration {
        by_class,
        pooled,
        slack,
        alpha,
        samples: all.len(),
    }
}

/// The conformal quantile, as a score floor.
///
/// Standard split-conformal index: with `n` calibration points, the rank is
/// `ceil((n+1)(1−α))`. When that exceeds `n` the sample is too small to support
/// the requested α and there is **no finite threshold** — returning `None` there
/// rather than clamping is the difference between a calibrated system and one
/// that merely looks calibrated.
fn conformal_floor(scores: &mut [f32], alpha: f32) -> Option<f32> {
    let n = scores.len();
    if n == 0 {
        return None;
    }
    let rank = (((n + 1) as f32) * (1.0 - alpha)).ceil();
    if rank > n as f32 {
        return None;
    }
    // Nonconformity is `1 − score`, so its upper quantile is the score's LOWER
    // quantile: sort ascending and take the (n − rank)th score.
    scores.sort_by(f32::total_cmp);
    let position = n.saturating_sub(rank as usize);
    scores.get(position).copied()
}

/// The same quantile for the margin, which is already a nonconformity (bigger is
/// worse), so it is the UPPER tail rather than the lower one.
fn conformal_slack(margins: &mut [f32], alpha: f32) -> Option<f32> {
    let n = margins.len();
    if n == 0 {
        return None;
    }
    let rank = (((n + 1) as f32) * (1.0 - alpha)).ceil();
    if rank > n as f32 {
        return None;
    }
    margins.sort_by(f32::total_cmp);
    margins.get(rank as usize - 1).copied()
}

/// Slack used when there is nothing to calibrate on. Conservative in the sense
/// that matters here: a small slack means small sets, which means the decision
/// falls back to "act only on a clear leader".
const UNCALIBRATED_SLACK: f32 = 0.05;

/// Total calibration queries across the whole installed set.
///
/// Sized by what the quantile needs, not by what is available: at α = 0.05 the
/// budget is split to 0.025 per condition, so the rank is ⌈(n+1)·0.975⌉ and any
/// n above ~40 supports it. Two thousand leaves three orders of magnitude of
/// headroom on resolution while keeping a rebuild in the tens of milliseconds.
const MAX_CALIBRATION_SAMPLES: usize = 2000;

/// Never thin an action below this, however many are installed. Per-class
/// (Mondrian) floors need their own samples, and an action reduced to a handful
/// silently falls back to the pooled floor — which is a correctness cliff, not a
/// gentle degradation.
const MIN_SAMPLES_PER_ACTION: usize = 12;

/// The corruption model: what happens to a declared phrase between the manifest
/// and the microphone.
///
/// Deliberately small and readable, because it is the assumption the whole
/// guarantee rests on. Widen it only with evidence from the action log, never to
/// make a number look better.
fn corrupt(clean: &str) -> Vec<String> {
    let words: Vec<&str> = clean.split(' ').filter(|w| !w.is_empty()).collect();
    if words.is_empty() {
        return Vec::new();
    }
    // The clean phrase belongs in the distribution: people do say the exact
    // thing, and pretending otherwise would bias the floor upward.
    let mut out = vec![clean.to_string()];

    // Fillers. Speech has them; declarations do not.
    out.push(format!("okay {clean}"));
    out.push(format!("{clean} please"));

    // A dropped SHORT word. The channel swallows "my", "the", "a", "to" — the
    // unstressed function words — and it does not silently delete content
    // words, it substitutes them (below). Modelling arbitrary deletion was
    // tried and is wrong twice over: it is not what ASR does, and covering it
    // demands a slack so wide that "play my gym playlist" and "play {artist}"
    // become indistinguishable, which is the exact discrimination the decision
    // layer exists to make.
    //
    // The cutoff is the same four characters `same_word` uses, and for the
    // mirror-image reason: words too short to spell-correct safely are exactly
    // the ones the channel drops.
    for skip in 0..words.len() {
        if words.len() == 1 || words[skip].len() >= 4 {
            continue;
        }
        let dropped: Vec<&str> = words
            .iter()
            .enumerate()
            .filter(|(i, _)| *i != skip)
            .map(|(_, w)| *w)
            .collect();
        out.push(dropped.join(" "));
    }

    // A spurious word inserted mid-utterance — a hesitation or a hallucinated
    // token. The matcher absorbs one between literals and cannot absorb one
    // *inside* a multi-token literal, which is exactly the point: without a
    // corruption the matcher genuinely struggles with, every margin is zero and
    // the slack quantile stops carrying information.
    for at in 1..words.len() {
        let mut variant: Vec<String> = words.iter().map(|w| (*w).to_string()).collect();
        variant.insert(at, "um".into());
        out.push(variant.join(" "));
    }

    // A one-character substitution in a word long enough for the matcher to
    // still recognise it. This is the case `same_word` exists for, so it has to
    // be in the calibration set or the tolerance is untested.
    for (index, word) in words.iter().enumerate() {
        if word.len() < 4 {
            continue;
        }
        let mut mangled: Vec<u8> = word.as_bytes().to_vec();
        let position = mangled.len() / 2;
        mangled[position] = if mangled[position] == b'a' {
            b'e'
        } else {
            b'a'
        };
        let Ok(mangled) = String::from_utf8(mangled) else {
            continue;
        };
        let mut variant: Vec<String> = words.iter().map(|w| (*w).to_string()).collect();
        variant[index] = mangled;
        out.push(variant.join(" "));
    }
    out
}

/// Turn a parsed template back into something sayable, substituting a neutral
/// filler for each placeholder. Used only for calibration.
fn render_template(template: &[grain_sdk::manifest::UtterancePart]) -> Option<String> {
    use grain_sdk::manifest::UtterancePart;
    let mut parts = Vec::new();
    for part in template {
        match part {
            UtterancePart::Literal(literal) => parts.push(literal.clone()),
            UtterancePart::Param(_) => parts.push("something".into()),
        }
    }
    (!parts.is_empty()).then(|| parts.join(" "))
}

// ── Provider selection ──────────────────────────────────────────────────────

/// Everything the ladder in PLAN §6 needs, supplied by the host so this stays a
/// pure function.
#[derive(Clone, Debug, Default)]
pub struct Preferences {
    /// Rung 2: domain → extension id.
    pub default_provider: HashMap<String, String>,
    /// Rung 4, tie-break only: which provider to prefer when two are otherwise
    /// of equal standing.
    ///
    /// The plan called this "the foreground app's extension", and that turned
    /// out to be unevaluable: **an extension never declares which application it
    /// is**. Spotify's extension is not the Spotify window, and inventing a
    /// mapping would mean either a new manifest field nobody needs or a guess
    /// from the extension's name.
    ///
    /// So the signal is **most recently used in this domain**, read from the
    /// action log. It is strictly better for the job anyway: it is a fact about
    /// what this user actually does rather than about which window happens to
    /// be focused, and the focused window was already the least stable rung on
    /// the ladder. Still a tie-break — it never overrules an explicit default.
    ///
    /// Keyed by domain, exactly like `default_provider`, because which domain
    /// wins is not known until after the decision — a single "most recent
    /// provider" would apply a music habit to a messaging request.
    pub recent_provider: HashMap<String, String>,
    /// Whether an Agent is available for escalation.
    pub agent_available: bool,
}

/// Prepositions after which a following name is the PROVIDER, not an entity.
///
/// Bare mid-utterance names stay entities, or "play Spotify Wrapped" and
/// "message Slack about the outage" break.
const PROVIDER_PREPOSITIONS: [&str; 5] = ["on", "in", "with", "using", "through"];

/// Strip a trailing "…on Spotify" and return `(remaining utterance, provider)`.
pub fn split_named_provider(spoken: &str, known: &[(String, String)]) -> (String, Option<String>) {
    let normalised = crate::action_router::normalise(spoken);
    let words: Vec<&str> = normalised.split(' ').filter(|w| !w.is_empty()).collect();
    for (name, id) in known {
        let name = crate::action_router::normalise(name);
        if name.is_empty() {
            continue;
        }
        let name_words: Vec<&str> = name.split(' ').collect();
        if words.len() <= name_words.len() {
            continue;
        }
        let start = words.len() - name_words.len();
        if words[start..] != name_words[..] {
            continue;
        }
        // Trailing standalone name, or one introduced by a preposition. A name
        // anywhere else is part of what the user is asking for.
        let (cut, matched) = if start > 0 && PROVIDER_PREPOSITIONS.contains(&words[start - 1]) {
            (start - 1, true)
        } else {
            (start, true)
        };
        if matched {
            return (words[..cut].join(" "), Some(id.clone()));
        }
    }
    (normalised, None)
}

// ── The decision ────────────────────────────────────────────────────────────

/// Decide what to do with one spoken request.
pub fn decide(
    spoken: &str,
    index: &ActionIndex,
    classes: &EquivalenceMap,
    calibration: &Calibration,
    preferences: &Preferences,
) -> Outcome {
    if crate::action_router::normalise(spoken).is_empty() {
        return Outcome::Refuse(RefuseReason::NothingHeard);
    }
    let ranked = index.rank(spoken);
    let lookup: HashMap<String, &IndexedAction> = index
        .actions()
        .iter()
        .map(|action| (action.qualified(), action))
        .collect();

    // Two filters, in order, because they answer different questions.
    //
    // 1. The absolute floor: is this in scope at all? Everything below it is
    //    evidence that nothing installed serves the request.
    // 2. The calibrated slack: given the leader, who else is genuinely
    //    plausible? Without this the set includes anything above the bar, and a
    //    0.65 also-ran turns a clear 0.95 winner into a question.
    let leader = ranked
        .iter()
        .find(|c| c.score >= calibration.floor(classes.class_of(&c.extension_id, &c.action_id)))
        .map(|c| c.score);
    let Some(leader) = leader else {
        return Outcome::Refuse(RefuseReason::NothingInstalledCanDoThat);
    };

    let mut best_per_class: BTreeMap<usize, Selection> = BTreeMap::new();
    let mut ungrouped: Vec<Selection> = Vec::new();
    for candidate in &ranked {
        let class = classes.class_of(&candidate.extension_id, &candidate.action_id);
        if candidate.score < calibration.floor(class) {
            continue;
        }
        if leader - candidate.score > calibration.slack {
            continue;
        }
        let Some(action) = lookup.get(&format!(
            "{}:{}",
            candidate.extension_id, candidate.action_id
        )) else {
            continue;
        };
        let selection = Selection::from(candidate, action);
        match class {
            Some(class) => {
                best_per_class.entry(class).or_insert(selection);
            }
            None => ungrouped.push(selection),
        }
    }

    let mut set: Vec<Selection> = best_per_class.into_values().chain(ungrouped).collect();
    set.sort_by(|a, b| b.score.total_cmp(&a.score));

    match set.len() {
        0 => Outcome::Refuse(RefuseReason::NothingInstalledCanDoThat),
        1 => resolve_single(set.remove(0), spoken, index, classes, preferences),
        n if n <= MAX_CLARIFY => Outcome::Choose {
            options: set,
            reason: ChooseReason::WhichAction,
        },
        // Too many plausible readings to ask an honest question about — CICC's
        // "no reasonable ground for a clarification question". A compound
        // request lands here too, which is where it belongs.
        _ if preferences.agent_available => {
            set.truncate(MAX_ESCALATE);
            Outcome::Escalate(set)
        }
        _ => Outcome::Refuse(RefuseReason::NeedsAgentButNoneConfigured),
    }
}

/// One class won. Now: is a required span missing, and which provider runs it?
fn resolve_single(
    winner: Selection,
    spoken: &str,
    index: &ActionIndex,
    classes: &EquivalenceMap,
    preferences: &Preferences,
) -> Outcome {
    if !winner.missing.is_empty() {
        return Outcome::Choose {
            options: vec![winner],
            reason: ChooseReason::MissingDetail,
        };
    }
    let Some(class) = classes.class_of(&winner.extension_id, &winner.action_id) else {
        return Outcome::Execute(winner);
    };

    // Everyone in the winning class who actually matched this utterance.
    let mut providers: Vec<Selection> = index
        .rank(spoken)
        .iter()
        .filter(|c| classes.class_of(&c.extension_id, &c.action_id) == Some(class))
        .filter_map(|c| {
            index
                .actions()
                .iter()
                .find(|a| a.extension_id == c.extension_id && a.action_id == c.action_id)
                .map(|a| Selection::from(c, a))
        })
        .collect();
    providers.dedup_by(|a, b| a.extension_id == b.extension_id);
    if providers.len() <= 1 {
        return Outcome::Execute(winner);
    }

    // Rung 2 — an explicit default for this domain.
    if let Some(default) = preferences.default_provider.get(&winner.domain) {
        if let Some(chosen) = providers.iter().find(|p| &p.extension_id == default) {
            return Outcome::Execute(chosen.clone());
        }
    }
    // Rung 4 — most recently used, and ONLY as a tie-break between providers of
    // equal standing. It never overrules an explicit default: a habit is
    // evidence, a setting is an instruction.
    if let Some(recent) = preferences.recent_provider.get(&winner.domain) {
        if let Some(chosen) = providers.iter().find(|p| &p.extension_id == recent) {
            return Outcome::Execute(chosen.clone());
        }
    }
    providers.truncate(MAX_CLARIFY);
    Outcome::Choose {
        options: providers,
        reason: ChooseReason::WhichProvider,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use grain_sdk::manifest::{ActionDecl, ActionParamDecl, ActionParamKind, LayerWhen};

    fn action(
        ext: &str,
        id: &str,
        domain: &str,
        risk: ActionRisk,
        utterances: &[&str],
        params: &[&str],
    ) -> IndexedAction {
        IndexedAction::from_decl(
            ext,
            &ActionDecl {
                id: id.into(),
                title: id.into(),
                domain: domain.into(),
                risk,
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
            },
        )
    }

    fn media_set() -> Vec<IndexedAction> {
        vec![
            action(
                "spotify",
                "next",
                "media",
                ActionRisk::Safe,
                &["skip this", "next song", "next track", "move on"],
                &[],
            ),
            action(
                "apple",
                "next",
                "media",
                ActionRisk::Safe,
                &["skip this", "next song", "next track", "move on"],
                &[],
            ),
            action(
                "spotify",
                "previous",
                "media",
                ActionRisk::Safe,
                &["go back", "previous song", "previous track", "back one"],
                &[],
            ),
            action(
                "slack",
                "send_dm",
                "messaging",
                ActionRisk::Confirm,
                &[
                    "tell {who} that {message}",
                    "message {who} saying {message}",
                    "send {who} a message saying {message}",
                ],
                &["who", "message"],
            ),
        ]
    }

    fn build(actions: Vec<IndexedAction>) -> (ActionIndex, EquivalenceMap, Calibration) {
        let classes = equivalence_classes(&actions);
        let index = ActionIndex::build(actions);
        let calibration = calibrate(&index, DEFAULT_ALPHA);
        (index, classes, calibration)
    }

    fn fixture() -> (ActionIndex, EquivalenceMap, Calibration) {
        build(media_set())
    }

    #[test]
    fn an_empty_capture_is_refused_not_escalated() {
        let (actions, classes, calibration) = fixture();
        let preferences = Preferences {
            agent_available: true,
            ..Default::default()
        };
        assert_eq!(
            decide("", &actions, &classes, &calibration, &preferences),
            Outcome::Refuse(RefuseReason::NothingHeard)
        );
        assert_eq!(
            decide("...", &actions, &classes, &calibration, &preferences),
            Outcome::Refuse(RefuseReason::NothingHeard)
        );
    }

    #[test]
    fn a_request_nothing_serves_says_so() {
        let (actions, classes, calibration) = fixture();
        for said in [
            "what's the weather",
            "turn the lights off",
            "so anyway I was thinking we should rewrite the parser this week",
        ] {
            assert_eq!(
                decide(
                    said,
                    &actions,
                    &classes,
                    &calibration,
                    &Preferences::default()
                ),
                Outcome::Refuse(RefuseReason::NothingInstalledCanDoThat),
                "\"{said}\" must not route"
            );
        }
    }

    #[test]
    fn one_provider_of_a_decided_request_just_runs() {
        let (actions, classes, calibration) = fixture();
        let outcome = decide(
            "go back",
            &actions,
            &classes,
            &calibration,
            &Preferences::default(),
        );
        match outcome {
            Outcome::Execute(selection) => {
                assert_eq!(selection.extension_id, "spotify");
                assert_eq!(selection.action_id, "previous");
                assert!(!selection.needs_confirmation());
            }
            other => panic!("expected Execute, got {other:?}"),
        }
    }

    #[test]
    fn two_providers_with_no_default_ask_which() {
        let (actions, classes, calibration) = fixture();
        let outcome = decide(
            "skip this",
            &actions,
            &classes,
            &calibration,
            &Preferences::default(),
        );
        match outcome {
            Outcome::Choose { options, reason } => {
                assert_eq!(reason, ChooseReason::WhichProvider);
                assert_eq!(options.len(), 2);
            }
            other => panic!("expected a provider chooser, got {other:?}"),
        }
    }

    #[test]
    fn an_explicit_default_ends_the_ladder() {
        let (actions, classes, calibration) = fixture();
        let preferences = Preferences {
            default_provider: HashMap::from([("media".to_string(), "apple".to_string())]),
            // The habit disagrees, and must lose: a habit is evidence, a
            // setting is an instruction.
            recent_provider: HashMap::from([("media".to_string(), "spotify".to_string())]),
            ..Default::default()
        };
        match decide("skip this", &actions, &classes, &calibration, &preferences) {
            Outcome::Execute(selection) => assert_eq!(selection.extension_id, "apple"),
            other => panic!("expected Execute via the default, got {other:?}"),
        }
    }

    #[test]
    fn a_habit_breaks_a_tie_but_only_a_tie() {
        let (actions, classes, calibration) = fixture();
        let preferences = Preferences {
            recent_provider: HashMap::from([("media".to_string(), "apple".to_string())]),
            ..Default::default()
        };
        match decide("skip this", &actions, &classes, &calibration, &preferences) {
            Outcome::Execute(selection) => assert_eq!(selection.extension_id, "apple"),
            other => panic!("expected the tie-break to resolve, got {other:?}"),
        }
    }

    #[test]
    fn a_habit_in_one_domain_does_not_leak_into_another() {
        // The reason rung 4 is keyed by domain rather than being a single
        // "most recent provider": the winning domain is not known until after
        // the decision, so a flat value would apply a music habit to a
        // messaging request.
        let (actions, classes, calibration) = fixture();
        let preferences = Preferences {
            recent_provider: HashMap::from([("messaging".to_string(), "apple".to_string())]),
            ..Default::default()
        };
        assert!(
            matches!(
                decide("skip this", &actions, &classes, &calibration, &preferences),
                Outcome::Choose { .. }
            ),
            "a habit from another domain must not resolve this tie"
        );
    }

    #[test]
    fn a_destructive_action_always_asks_however_well_it_scored() {
        let (actions, classes, calibration) = fixture();
        match decide(
            "tell Jack that I am running late",
            &actions,
            &classes,
            &calibration,
            &Preferences::default(),
        ) {
            Outcome::Execute(selection) => {
                assert_eq!(selection.action_id, "send_dm");
                assert_eq!(selection.score, 1.0_f32.min(selection.score));
                assert!(
                    selection.needs_confirmation(),
                    "the adversary is ASR substitution, not the router — no score retires the \
                     read-back"
                );
                assert_eq!(selection.spans.get("who").map(String::as_str), Some("jack"));
            }
            other => panic!("expected Execute pending confirmation, got {other:?}"),
        }
    }

    #[test]
    fn a_missing_required_span_asks_rather_than_guessing() {
        let mut actions = media_set();
        actions.push(action(
            "spotify",
            "play_artist",
            "media",
            ActionRisk::Safe,
            &["play {artist}", "put on some {artist}", "put on {artist}"],
            &["artist"],
        ));
        let (index, classes, calibration) = build(actions);
        match decide(
            "play",
            &index,
            &classes,
            &calibration,
            &Preferences::default(),
        ) {
            Outcome::Choose { reason, .. } => assert_eq!(reason, ChooseReason::MissingDetail),
            Outcome::Refuse(_) => {}
            other => panic!("a bare verb must never execute: {other:?}"),
        }
    }

    #[test]
    fn calibration_refuses_to_invent_a_threshold_it_cannot_support() {
        // With five samples there is no finite 95% conformal quantile. Clamping
        // would look calibrated and not be.
        let mut five = vec![0.9, 0.8, 0.7, 0.6, 0.5];
        assert_eq!(super::conformal_floor(&mut five, 0.05), None);
        // Twenty supports it.
        let mut twenty: Vec<f32> = (0..20).map(|i| 0.5 + i as f32 * 0.02).collect();
        assert!(super::conformal_floor(&mut twenty, 0.05).is_some());
    }

    #[test]
    fn a_tighter_miscoverage_budget_lowers_the_bar_and_asks_more() {
        // The direction that is easy to get backwards, and that this codebase
        // got backwards in its first draft. Smaller alpha = higher coverage =
        // the intended action must be in the set more often = a LOWER floor and
        // LARGER sets, so more clarification questions rather than fewer.
        //
        // If this monotonicity ever breaks, the quantile has stopped being a
        // quantile and every number downstream is decoration.
        let index = ActionIndex::build(media_set());
        // Both budgets must be SUPPORTED by the sample count, or the comparison
        // is between a quantile and the uncalibrated fallback — which is not a
        // monotonicity failure, it is the small-n refusal working. (α = 0.01 is
        // genuinely unsupported at this fixture's size, and that is checked
        // separately below.)
        let strict = calibrate(&index, 0.05);
        let loose = calibrate(&index, 0.30);
        assert!(
            strict.pooled <= loose.pooled,
            "tighter coverage must not raise the bar ({} vs {})",
            strict.pooled,
            loose.pooled
        );
        // And the refusal itself: a budget this fixture cannot support falls
        // back rather than pretending.
        assert_eq!(calibrate(&index, 0.001).pooled, super::UNCALIBRATED_FLOOR);
    }

    /// Build a deliberately adversarial installed set: every action shares its
    /// vocabulary with every other, so the inverted index prunes nothing. Real
    /// vocabularies are far more diverse; this is the worst case, not the case.
    fn crowded(extensions: usize, per_extension: usize) -> Vec<IndexedAction> {
        let mut actions = Vec::new();
        for extension in 0..extensions {
            for id in 0..per_extension {
                let utterances: Vec<String> = (0..12)
                    .map(|phrasing| format!("command {extension} number {id} way {phrasing}"))
                    .collect();
                let borrowed: Vec<&str> = utterances.iter().map(String::as_str).collect();
                actions.push(action(
                    &format!("ext{extension}"),
                    &format!("act{id}"),
                    "media",
                    ActionRisk::Safe,
                    &borrowed,
                    &[],
                ));
            }
        }
        actions
    }

    #[test]
    fn building_the_index_is_fast_even_when_calibrating_is_not() {
        // The split that makes the background pass possible: the host must be
        // able to build an index and start routing conservatively without
        // waiting for a quantile. If this ever becomes slow, flipping a switch
        // stalls.
        let actions = crowded(20, 15);
        let started = std::time::Instant::now();
        let index = ActionIndex::build(actions);
        let elapsed = started.elapsed();
        assert_eq!(index.actions().len(), 300);
        assert!(
            elapsed.as_millis() < 500,
            "building a 300-action index took {elapsed:?}; it is on the rebuild path"
        );
    }

    #[test]
    fn calibration_stays_bounded_on_a_large_installed_set() {
        // Bounds the SAMPLE count, not the wall clock — calibration runs in the
        // background precisely because its cost grows with the installed set.
        // What must not happen is the sample budget quietly escaping, which is
        // what turns seconds into minutes.
        let index = ActionIndex::build(crowded(20, 15));
        let calibration = calibrate(&index, DEFAULT_ALPHA);
        assert!(
            calibration.samples <= MAX_CALIBRATION_SAMPLES,
            "sampling budget escaped: {} samples",
            calibration.samples
        );
        assert!(calibration.is_calibrated());
    }

    #[test]
    fn a_conservative_calibration_only_lets_a_near_exact_match_through() {
        // What routing uses while the background pass is still running. It has
        // to be safe rather than merely present: high floor, no slack, so the
        // degradation is "asks more often for a moment", never "acts on a
        // guess".
        let index = ActionIndex::build(media_set());
        let classes = equivalence_classes(index.actions());
        let waiting = Calibration::conservative();
        assert!(!waiting.is_calibrated());

        // A declared phrase, said exactly, still runs.
        assert!(matches!(
            decide(
                "go back",
                &index,
                &classes,
                &waiting,
                &Preferences::default()
            ),
            Outcome::Execute(_)
        ));
        // Anything softer waits for the real thresholds rather than guessing.
        assert!(matches!(
            decide(
                "tell Jack that I am running late",
                &index,
                &classes,
                &waiting,
                &Preferences::default()
            ),
            Outcome::Refuse(_)
        ));
    }

    #[test]
    fn a_confusable_installed_set_asks_instead_of_picking() {
        // On a well-separated set the slack calibrates to **zero**, and that is
        // the mechanism working rather than failing: under the corruption model
        // the right action always leads, so no slack is needed to cover it.
        //
        // It would be easy to read that as "the margin test is inert" and reach
        // for a hand-picked minimum. It is not inert — genuinely confusable
        // actions are *ties*, not near-misses, and a tie is inside any slack
        // including zero. The property worth asserting is the outcome, not the
        // number: three different actions that all legitimately answer to
        // "skip this" must produce a question rather than whichever one sorted
        // first.
        assert_eq!(
            calibrate(&ActionIndex::build(media_set()), DEFAULT_ALPHA).slack(),
            0.0
        );

        let mut confusable = media_set();
        confusable.push(action(
            "deck",
            "next_slide",
            "system",
            ActionRisk::Safe,
            &["next slide", "next one", "skip this", "move on"],
            &[],
        ));
        confusable.push(action(
            "reader",
            "next_page",
            "files",
            ActionRisk::Safe,
            &["next page", "next one", "skip this", "move on"],
            &[],
        ));
        let (index, classes, calibration) = build(confusable);
        match decide(
            "skip this",
            &index,
            &classes,
            &calibration,
            &Preferences::default(),
        ) {
            Outcome::Choose { options, .. } => assert!(
                options.len() >= 3,
                "every action that answers to this phrase has to be on the ballot"
            ),
            other => panic!("a three-way collision must never just run: {other:?}"),
        }
    }

    #[test]
    fn the_slack_keeps_an_also_ran_out_of_the_question() {
        // An absolute floor alone builds sets that are far too generous: a 0.65
        // match sitting alongside a 0.95 one turned a clear winner into a
        // question. The calibrated margin is what closes that, and it has to
        // stay tighter than the gap between a one-literal and a three-literal
        // template or the whole decision layer goes back to asking constantly.
        let mut actions = media_set();
        actions.push(action(
            "spotify",
            "play_artist",
            "media",
            ActionRisk::Safe,
            &["play {artist}", "put on some {artist}", "put on {artist}"],
            &["artist"],
        ));
        actions.push(action(
            "spotify",
            "play_playlist",
            "media",
            ActionRisk::Safe,
            &[
                "play my {playlist} playlist",
                "start my {playlist} playlist",
                "put on my {playlist} playlist",
            ],
            &["playlist"],
        ));
        let (index, classes, calibration) = build(actions);
        match decide(
            "play my gym playlist",
            &index,
            &classes,
            &calibration,
            &Preferences::default(),
        ) {
            Outcome::Execute(selection) => {
                assert_eq!(selection.action_id, "play_playlist");
                assert_eq!(
                    selection.spans.get("playlist").map(String::as_str),
                    Some("gym")
                );
            }
            other => panic!("a clear leader must not become a question: {other:?}"),
        }
    }

    #[test]
    fn calibration_is_measured_against_speech_not_against_synonyms() {
        // The corruption model is the assumption the guarantee rests on, so
        // every case is pinned. If someone widens or narrows this to make a
        // number look better, this test is where they have to say so out loud.
        let variants = super::corrupt("play my gym playlist");
        // The clean phrase belongs in the distribution — people do say the
        // exact thing, and omitting it would bias the floor upward.
        assert!(variants.contains(&"play my gym playlist".to_string()));
        // Filler, front and back.
        assert!(variants.iter().any(|v| v.starts_with("okay ")));
        assert!(variants.iter().any(|v| v.ends_with(" please")));
        // A dropped SHORT word — and only a short one. The channel swallows
        // "my"; it substitutes content words rather than deleting them.
        assert!(variants.iter().any(|v| v == "play gym playlist"));
        assert!(
            !variants.iter().any(|v| v == "my gym playlist"),
            "dropping a content word is not what ASR does, and modelling it \
             forces a slack wide enough to swallow every real distinction"
        );
        // An inserted hesitation.
        assert!(variants.iter().any(|v| v.contains(" um ")));
        // One mangled character: same shape, one word spelled wrong.
        assert!(
            variants.iter().any(|v| {
                let words: Vec<&str> = v.split(' ').collect();
                words.len() == 4 && v != "play my gym playlist" && !v.contains("um")
            }),
            "the substitution case is missing: {variants:?}"
        );
    }

    #[test]
    fn a_named_provider_is_taken_off_the_end_of_the_utterance() {
        let known = vec![
            ("Spotify".to_string(), "spotify".to_string()),
            ("Apple Music".to_string(), "apple".to_string()),
        ];
        assert_eq!(
            split_named_provider("skip this on Spotify", &known),
            ("skip this".into(), Some("spotify".into()))
        );
        assert_eq!(
            split_named_provider("next song with Apple Music", &known),
            ("next song".into(), Some("apple".into()))
        );
    }

    #[test]
    fn a_provider_name_in_the_middle_of_a_request_stays_an_entity() {
        // "Play Spotify Wrapped" must not become "play" on Spotify.
        let known = vec![("Spotify".to_string(), "spotify".to_string())];
        assert_eq!(
            split_named_provider("play Spotify Wrapped", &known),
            ("play spotify wrapped".into(), None)
        );
    }
}
