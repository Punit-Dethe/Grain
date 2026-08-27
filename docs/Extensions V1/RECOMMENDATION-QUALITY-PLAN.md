# Extension Recommendation Quality Plan

> **Architecture transition (2026-08-28):** This is the V1 recommendation baseline. Reuse its corpus and evaluator for Extensions 2.0 action Recall@K, hot-set, and fallback-search testing; do not expand the single-extension routing product model. See `docs/Extensions 2.0/PLAN.md`.

**Status:** research-backed implementation plan

**Date:** 2026-08-25

**Scope:** Grain choosing a searchable extension from a completed speech request. This does not cover how an extension interprets its own commands, and it does not reopen the confirmation-surface design.

## 1. Decision

Stop optimizing the generic internal-command router. Extension recommendation is the valuable problem, and it is a different problem:

- the set of installed extensions changes at runtime;
- each extension begins with few author examples;
- a request may match no extension;
- nearby extensions can be semantically almost identical;
- the input is an ASR hypothesis, not clean typed text;
- a wrong high-confidence route is materially worse than showing two choices.

Treat recommendation as **open-set, selective retrieval**. Retrieve and rank likely extensions, calibrate whether the evidence is sufficient, then return either one candidate, a small clarification set, or no match. Exact extension-name recognition remains a separate structural signal.

The target is a small multi-stage ranker, not a single cosine comparison:

```text
final ASR hypothesis
        |
        +--> exact/ASR-tolerant name and alias recognition --------+
        |                                                          |
        +--> lexical fields (examples, purpose, entities) --+       |
        |                                                   |       |
        +--> dense text matching over route examples -------+--> feature fusion
                                                                    |
                                                    calibrated open-set decision
                                                       /          |          \
                                                no match     2-3 choices     one top
                                                                              |
                                                                  separate Auto-send gate
```

“RAG” is useful here only in its retrieval sense: extension manifests form a tiny local corpus. Adding a generative model after retrieval would not make the ranking evidence better, and would add RAM, latency, nondeterminism, and a new prompt-injection surface. A bounded reranker may be tested later; generation is not part of the recommendation path.

## 2. Audit of the current implementation

| Layer | Current implementation | Finding |
|---|---|---|
| Searchable contract | `recommend.purpose`, `examples`, `aliases`, and `entities` | Good manifest shape, but only aliases and examples affect ranking. Purpose and entities are carried/display/consent metadata today. |
| Named route | `IndexedRecommendation` checks normalized contiguous alias token runs | Correct to keep separate from topical similarity. Aliases need stronger store policy because they have privileged precedence. |
| Dense representation | `BAAI/bge-small-en-v1.5`, 384 dimensions, query prefixed with its retrieval instruction | The prefix is intended for short-query-to-long-passage retrieval. Grain compares short utterances with short utterances, so prefixed versus symmetric embedding has never been validated for this task. |
| Per-extension score | Maximum cosine across all author examples | One lucky or overly broad example wins. More examples create more chances to win even under the existing cap. There is no coverage/consistency signal. |
| Filtering | Global `TOPICAL_FLOOR = 0.50`, copied from Grain Space recall | This is not calibrated for routing. The BGE model card explicitly warns that a score above 0.5 does not imply similarity and says downstream thresholds must be selected on task data. |
| Ambiguity | Top score and top-to-runner-up margin; fixed `0.15` Auto-send margin | Structurally useful, but neither value is calibrated on cross-extension data. |
| Feedback | Bounded decisions/declines per extension apply a capped topical penalty | Privacy-preserving and safe, but too broad: confusion between GitHub and Linear should not punish GitHub against Spotify. |
| Evaluation | `--eval` loads one manifest and ranks `contributes.actions` | It evaluates internal commands, not Grain's cross-extension recommendation, despite the V1 plan promising recommendation rank and Auto-send evidence. This is the largest verification gap. |
| ASR bias | Extension Mode seeds the recognizer from internal action literals | Searchable extension names and approved spoken aliases are not the primary vocabulary, even though they are the strongest recommendation signal. |
| Security boundary | Transcript stays host-side until accepted; examples are capped and recommendation approval is fingerprint-bound | Strong foundation. Ranking quality still needs defenses against generic alias claims and example-set gaming. |

Relevant implementation points:

- `src-tauri/src/extension_host.rs`: `collect_recommendation`, `semantic_scores`, and `action_vocabulary`
- `src-tauri/src/grain_space/embed.rs`: `embed_query` and `QUERY_INSTRUCTION`
- `crates/grain-core/src/recommend.rs`: `TOPICAL_FLOOR`, `rank`, and `auto_send_target`
- `src-tauri/src/extension_misroutes.rs`: global per-extension penalty
- `src-tauri/src/grain_eval.rs`: command-only golden-file entry point

## 3. What the research changes

### 3.1 Model scores are features, not confidence

The current `0.50` cutoff is not defensible. BGE's own model card says its similarities commonly occupy a high range, that `> 0.5` does not mean two sentences are similar, and that filtering thresholds must be chosen on downstream data. Grain must learn or calibrate its operating point from held-out Grain routes, including no-match utterances.

The query instruction is also not sacred. BGE recommends it for a short query retrieving a longer passage and says other cases need no instruction. Recommendation is symmetric utterance matching. The first same-model experiment must compare:

1. current asymmetric/prefixed query;
2. symmetric unprefixed utterance embeddings;
3. structured extension documents, only if a retrieval adapter is used.

### 3.2 The model must be selected for this task

MTEB separates retrieval, classification, clustering, reranking, and semantic-textual-similarity tasks because model quality is task-specific. General retrieval leaderboards cannot choose Grain's model. The bake-off must use Grain's recommendation corpus and exact deployable artifacts.

`jina-embeddings-v5-text-nano-text-matching` is a strong candidate:

- a task-specific **text-matching** adapter fits utterance-to-utterance comparison better than a passage-retrieval prompt;
- it is multilingual, 239M parameters, 768 dimensions, and supports Matryoshka truncation;
- the official GGUF Q8 artifact is 233 MB; Q4_K_M is 157 MB;
- the base model and GGUF artifacts are **CC BY-NC 4.0**. Grain cannot distribute them commercially without a separate license.

Therefore Jina v5 Nano Q8 and Q4 belong in the benchmark, but are blocked from product selection until licensing is resolved. Weight quantization and output-vector quantization are different; the paper's robustness claims do not replace testing each exact GGUF artifact.

The initial model matrix is:

| Candidate | Why test it | Gate |
|---|---|---|
| Current BGE-small, prefixed | Reproducible baseline | None |
| Current BGE-small, unprefixed | Same RAM/model with geometry better aligned to symmetric matching | Must beat the baseline on hard pairs and no-match rejection |
| Jina v5 Nano Text-Matching Q8 | User-identified, task-specific, multilingual quality candidate | Commercial distribution license; measured RSS/cold and warm latency |
| Jina v5 Nano Text-Matching Q4_K_M | Similar disk footprint to the current model, potentially better quality/RAM point | Same license gate; no material quality loss versus Q8 |
| `all-MiniLM-L6-v2` | Apache-2.0, 22.7M-parameter symmetric sentence-similarity control; tests how far a much smaller model can go | English-only; must earn inclusion on Grain hard pairs |
| `e5-small-v2` with symmetric `query:` inputs | MIT, 33.4M parameters like current BGE, and explicitly documents symmetric semantic-similarity use | English-only; exact ONNX/runtime artifact required |
| `nomic-embed-text-v1.5` classification and symmetric variants | Apache-2.0, ~0.1B parameters, task prefixes and Matryoshka dimensions | Compare task prefixes; no unreviewed `trust_remote_code` path in production |

Do not select a model from aggregate MTEB alone. Select the Pareto winner on Grain top-1 quality, hard-negative errors, no-match errors, ASR degradation, latency, peak RSS, resident RSS, artifact size, platform support, and license.

### 3.3 Dense-only and lexical-only each discard useful evidence

Exact names, brands, uncommon nouns, and ASR-near spellings are often best handled lexically; paraphrases and indirect requests are best handled semantically. Production semantic routers expose multiple lexical and embedding signals with per-signal thresholds and aggregation. Hybrid-retrieval research supports combining the complementary rankings.

Grain should compare two fusion baselines:

- Reciprocal Rank Fusion, which combines ranks without pretending unlike scores share a scale;
- a learned convex/logistic fusion over normalized features, which research shows can outperform RRF with a small calibration set.

The production candidate should be a tiny, monotonic, inspectable scorer—not a new neural service. Candidate features:

- exact named hit and alias specificity;
- word BM25/IDF score;
- character n-gram/fuzzy score for ASR spelling errors;
- best dense example score;
- mean of the best two or three dense scores;
- distance to a bounded extension prototype or two mode prototypes;
- purpose and entity match scores;
- number of examples, used for normalization rather than as an advantage;
- top-to-second extension margin;
- bounded pairwise feedback adjustment.

Every feature family and aggregation rule stays behind the evaluation harness. No hand-authored score blend ships merely because it looks plausible.

### 3.4 Examples need hard negatives and coverage, not volume

DPR, BGE fine-tuning guidance, and tool-retrieval systems all make hard negatives central: training or evaluation must show the model realistic near misses, not only unrelated examples. Grain should mine the nearest examples belonging to competing extensions and maintain a collision matrix.

For each extension, compare these aggregations:

- `max` (current baseline);
- top-2/top-3 mean;
- one normalized prototype/centroid;
- at most two or three prototypes for multi-modal extensions;
- a learned aggregate of max, top-k mean, and prototype distance.

The maximum is expected to retain recall but be easiest to game. A single centroid is cheap but can blur an extension with several unrelated capabilities. Bounded multi-prototypes or a feature combination preserve modes without growing an engine.

Manifest examples remain phrases a person might actually say. Store CI should reject duplicates, near-duplicates, generic boilerplate, and examples that collide broadly. Optional author-supplied negative examples should be added only if the evaluation proves that they improve held-out performance; a store-generated collision report is preferable to expanding the public runtime contract prematurely.

### 3.5 Abstention is a product feature

CLINC150 demonstrates that systems which classify in-scope intents well can still struggle to recognize out-of-scope requests. Grain must not assume every Extension Mode request belongs to an installed extension.

CICC applies conformal prediction to produce small clarification sets with a target coverage and extends the method to out-of-scope detection. Grain already has the correct product surface—a chooser—so the ranker should optimize the size and reliability of that set instead of forcing a winner.

The decision layer should use held-out calibration data to return:

- no match when all candidates are below the open-set boundary;
- a small candidate set when several extensions remain plausible;
- one highlighted candidate when it is clearly separated;
- Auto-send only through its stricter, separately measured policy.

Start with calibrated top-score and margin rules. Add conformal candidate sets only after the dataset is large enough and the exchangeability assumptions are documented. Report risk-versus-coverage, not accuracy alone.

### 3.6 Speech errors must be first-class test data

Clean-text intent datasets are insufficient. SLURP contains real speech and ASR baselines; SpokenCSE explicitly derives ASR hypotheses and uses contrastive learning to improve spoken-language robustness.

Near term:

- bias Extension Mode toward installed extension names and approved spoken aliases, within the existing hotword budget;
- collect opt-in or developer-test pairs of reference text and Grain's own ASR hypothesis;
- generate deterministic error variants from measured Grain confusions, not generic typo noise alone;
- evaluate clean and ASR versions as paired cases;
- include filler, self-correction, homophones, product names, code-switching, and indirect phrasing.

Later, if model-independent fixes plateau, use clean/ASR pairs as contrastive positives and nearest competing extension phrases as hard negatives in an offline fine-tune. Runtime must still support dynamically installed extensions through example/prototype retrieval rather than a fixed class head.

## 4. Evaluation contract

No ranker or model change lands before a cross-extension harness exists.

### 4.1 Corpus

Create a versioned repository-owned golden corpus with disjoint author/index, calibration, and test splits. It must include:

- realistic pools of 5, 24, 50, and 100 installed searchable extensions;
- real names such as GitHub and Spotify, plus close substitutes such as GitLab, Linear, YouTube Music, and a local music library;
- broad and narrow extensions;
- multiple phrasing styles per intent, written independently of manifest examples;
- explicit-name, implicit-topic, multi-intent, ambiguous, and no-match cases;
- hard pairs mined from the current ranker's nearest neighbors;
- clean text paired with actual or simulated Grain ASR hypotheses;
- varying example counts, including an adversarial extension using the maximum allowed count;
- English now and a separately reported multilingual slice for multilingual candidate models.

Public datasets are stress-test inputs, not substitutes for Grain data:

- CLINC150 for open-set/no-match behavior;
- Banking77 for fine-grained near-neighbor intent separation;
- SLURP/Speech-MASSIVE for speech and ASR robustness;
- MASSIVE for multilingual intent coverage.

### 4.2 Metrics

Report every metric by pool size, clean/ASR, named/topical, hard-negative, and no-match slices:

- top-1 accuracy, recall@3, and mean reciprocal rank;
- no-match precision, recall, and false-recommendation rate;
- pairwise confusion matrix and worst confused extension pairs;
- risk-coverage curve;
- candidate-set coverage and median/95th-percentile set size;
- calibration error/Brier score once scores claim to be probabilities;
- clean-to-ASR quality drop;
- wrong Auto-send count and a confidence interval, not only its percentage;
- cold and warm p50/p95 latency, index-build time, artifact size, peak RSS, and resident RSS.

Use bootstrap confidence intervals for model/ranker comparisons. A candidate wins only when the improvement is outside noise on the hard-negative and no-match slices, not merely on aggregate accuracy.

### 4.3 Initial quality gates

These are product targets, to be revisited only with measured evidence:

- named-route recall at least 99% on approved names/aliases, with no generic-alias collision;
- in-scope recall@3 at least 98%;
- no-match false-recommendation rate at most 2%;
- hard-negative top-1 accuracy at least 90%;
- median clarification set no larger than 2 and 95th percentile no larger than 3;
- clean-to-ASR top-1 degradation no more than 5 percentage points;
- zero observed wrong Auto-sends in the release corpus and beta, with graduation additionally requiring a sufficiently tight statistical upper bound;
- no new always-resident engine; all model memory follows the existing witness/TTL lifecycle;
- a new model must justify every increase in RSS and artifact size with statistically reliable Grain-quality gains.

## 5. Target implementation phases

### RQ0 — Correct measurement

1. Add a recommendation mode to the headless eval path. Its input describes an installed extension pool plus held-out requests and expected extension(s) or `none`.
2. Reuse the same pure ranker and same exact model artifacts as the live app.
3. Add the metrics and slices in section 4, JSON output, deterministic seeds, corpus schema versioning, and CI regression comparison.
4. Record the current BGE-prefixed/max/0.50 baseline without changing production behavior.

**Exit:** one command reproduces current cross-extension recommendation quality and exposes every current false positive/negative.

**Implementation status (2026-08-25): RQ0 foundation complete.** The headless
app now accepts a strict, bounded, versioned cross-extension corpus, invokes the
same production name detector, embedder, max-example scoring, ranker, and
Auto-send policy, and emits human or JSON metrics with per-slice/confusion and
operating-point reports. Optional corpus-owned quality gates provide a direct CI
exit code. The checked-in five-extension hard pool records the unchanged current
baseline at 96% overall top-1, 100% in-scope top-1 and recall@3, 80% no-match
recall, and zero wrong Auto-sends. The remaining RQ0 data work is to add the
24/50/100-extension pools, independent calibration/test splits, and a model-
provisioned CI runner; those are corpus/infrastructure additions rather than
ranker changes.

### RQ1 — Same-model architecture bake-off

Implement as interchangeable pure scoring strategies:

1. prefixed versus symmetric BGE queries;
2. max, top-k mean, centroid, and bounded multi-prototype aggregation;
3. examples-only versus examples + purpose + entities;
4. lexical BM25/IDF + character n-gram retrieval;
5. RRF versus a fitted monotonic/logistic fusion;
6. global versus calibrated pool-aware threshold and margin.

Do not change the manifest yet. Purpose/entities can be consumed from the existing approved contract.

**Exit:** select the simplest statistically superior strategy. If no strategy wins, preserve the baseline and retain the evidence.

### RQ2 — Model bake-off

1. Add an offline benchmark adapter for exact pinned artifacts.
2. Benchmark BGE variants, Jina v5 Nano Text-Matching Q8/Q4, and at least one permissively licensed small symmetric encoder.
3. Test 768, 384, 256, and smaller supported Matryoshka output dimensions where applicable; vector truncation affects index/cache memory, not model weight memory.
4. Measure every supported OS/CPU target, including first-use download and cancellation.
5. Complete license and supply-chain review: redistribution terms, immutable revision, hashes/signatures, runtime parser, and update/rollback behavior.

**Exit:** an evidence-backed Pareto frontier. Jina cannot be selected for commercial Grain without a compatible license.

### RQ3 — Selective decision and safe Auto-send

1. Fit the open-set boundary on the calibration split, using both score and separation.
2. Produce one, few, or none rather than forcing a fixed top three.
3. Compare simple calibrated sets with CICC/conformal sets.
4. Calibrate separately by model artifact and ranking strategy; invalidate calibration when either changes.
5. Tune Auto-send separately on the highest-risk target and keep it beta until the real-extension evidence gate is met.

**Exit:** quality gates in section 4.3 pass on untouched test data; chooser set sizes and risk-coverage are acceptable.

### RQ4 — ASR, feedback, and store hardening

1. Replace action-literal-first Extension Mode bias with a bounded priority list led by searchable names and approved spoken aliases; retain useful action words only if the budget allows.
2. Add paired clean/ASR evaluation and measured confusion augmentation.
3. Replace the broad decline penalty with bounded, decayed pairwise evidence: “A was offered, B was selected” or “A declined, then B selected.” Apply it only when that pair competes again.
4. Never persist request text or request embeddings. Exact named hits remain immune to learned penalties.
5. Add store CI collision reports, alias-policy enforcement, example quality checks, and pool-level recommendation evaluation.

**Exit:** ASR and adversarial/store slices pass; feedback improves repeated pairwise errors without degrading unrelated routes.

### RQ5 — Optional learned ranking

Only begin if RQ1–RQ4 plateau below the quality gates:

1. contrastively fine-tune a small encoder offline with positive paraphrases, clean/ASR pairs, and mined hard negatives;
2. keep runtime classes dynamic by continuing to score extension examples/prototypes;
3. alternatively train a tiny global projection or monotonic feature ranker over frozen embeddings;
4. test a top-k cross-encoder only if it can reuse an existing artifact or its measured gain justifies another model lifecycle.

SetFit/prototypical-network research supports few-shot representation learning and class prototypes, but a fixed classifier head is a poor match for extensions installed after release. The reusable outcome must remain an encoder/ranker, not a baked list of extension classes.

## 6. Security, privacy, and abuse requirements

- An extension never receives the transcript merely because it ranked; current accept-before-hand-off remains invariant.
- Auto-send remains opt-in at global, author, and per-extension levels and is evaluated as a higher-risk policy.
- Aliases are privileged routing claims. Store validation must restrict them to the extension name, brand, acronym, or reviewed spoken variants; generic terms such as “music” cannot be exclusive named aliases.
- Normalized name/alias collisions are rejected or explicitly adjudicated. Registry order must never decide a privileged tie.
- Example caps remain. Ranking must normalize for example count so using the cap cannot create a statistical advantage.
- Manifest text is untrusted bounded data. It is never interpreted as a prompt and is escaped at every UI/log boundary.
- Raw transcripts and embeddings are not stored for learning. Pairwise counters contain IDs and bounded counts only, decay, and are purged with the extension.
- Model artifacts are pinned and integrity-checked. A model/license update invalidates cached vectors and calibration data.
- Evaluation corpora containing user speech must be opt-in, redacted, versioned, and distributable under explicit terms.
- Ranking remains local. No network call is introduced on the hot path.

## 7. Explicit non-goals

- No generic internal-command router work in this program.
- No LLM or generated answer in recommendation.
- No graph database: the corpus is too small and flat for graph retrieval to add useful evidence.
- No online bandit that deliberately explores wrong routes.
- No transcript or embedding telemetry by default.
- No per-extension threshold authored by guesswork.
- No model swap based only on public leaderboard averages.
- No second persistent inference engine before the measured need exists.

## 8. Recommended first implementation slice

RQ0 is the only responsible first step. The existing V1 plan incorrectly marks evaluation complete for recommendation, while the shipped harness evaluates commands. Build the cross-extension corpus/harness, preserve today's output as the baseline, and only then run the same-model and Jina bake-offs.

The likely high-leverage path, subject to that evidence, is:

1. symmetric text-matching embeddings;
2. hybrid lexical + dense retrieval;
3. top-k/prototype features instead of max-only;
4. a tiny learned/calibrated fusion layer;
5. open-set abstention and a small clarification set;
6. ASR-specific evaluation and pairwise hard negatives.

This changes the ranking stack substantially without adding a generative model or an always-resident service.

## 9. Primary sources and production references

### Models and evaluation

- [BGE-small-en-v1.5 model card](https://huggingface.co/BAAI/bge-small-en-v1.5) — query instructions, score-distribution warning, hard-negative guidance, and task-specific MTEB results.
- [jina-embeddings-v5-text technical report](https://arxiv.org/abs/2602.15547) and [official model card](https://huggingface.co/jinaai/jina-embeddings-v5-text-nano) — task-targeted adapters, multilingual results, dimensions, and license.
- [Jina v5 Nano Text-Matching GGUF](https://huggingface.co/jinaai/jina-embeddings-v5-text-nano-text-matching-GGUF) — exact Q4/Q8 artifacts, sizes, llama.cpp use, and CC BY-NC license.
- [MTEB paper](https://arxiv.org/abs/2210.07316) and [official repository](https://github.com/embeddings-benchmark/mteb) — task-specific embedding evaluation.
- [all-MiniLM-L6-v2 model card](https://huggingface.co/sentence-transformers/all-MiniLM-L6-v2) — small Apache-2.0 symmetric sentence encoder.
- [E5-small-v2 model card](https://huggingface.co/intfloat/e5-small-v2) — MIT model, symmetric-task prefix guidance, and absolute-score warning.
- [Nomic Embed v1.5 model card](https://huggingface.co/nomic-ai/nomic-embed-text-v1.5) — Apache-2.0 task prefixes and Matryoshka dimensions.

### Ranking, fusion, and hard negatives

- [Dense Passage Retrieval](https://aclanthology.org/2020.emnlp-main.550/) and [official DPR repository](https://github.com/facebookresearch/DPR) — positive, negative, and hard-negative dense retrieval training.
- [Reciprocal Rank Fusion](https://research.google/pubs/reciprocal-rank-fusion-outperforms-condorcet-and-individual-rank-learning-methods/) — rank-based multi-retriever fusion.
- [Analysis of Fusion Functions for Hybrid Retrieval](https://arxiv.org/abs/2210.11934) — RRF versus learned convex score fusion.
- [Semantic Router](https://github.com/aurelio-labs/semantic-router) and [vLLM Semantic Router configuration](https://github.com/vllm-project/semantic-router/blob/main/config/config.yaml) — production examples of utterance routes, optimization, lexical/dense signals, aggregation, and per-route thresholds.
- [ToolBench/ToolLLM](https://github.com/OpenBMB/ToolBench) and [paper](https://arxiv.org/abs/2307.16789) — separate large-catalog tool retrieval and evaluation.

### Open set, clarification, and few-shot learning

- [Conformal Intent Classification and Clarification](https://aclanthology.org/2024.findings-naacl.156/) and [official code](https://github.com/florisdenhengst/cicc) — small clarification sets and out-of-scope detection.
- [CLINC150](https://aclanthology.org/D19-1131/) — realistic out-of-scope intent evaluation.
- [Banking77](https://aclanthology.org/2020.nlp4convai-1.5/) — fine-grained, few-shot intent separation.
- [SetFit](https://arxiv.org/abs/2209.11055) and [official repository](https://github.com/huggingface/setfit) — efficient few-shot contrastive fine-tuning.
- [Prototypical Networks](https://arxiv.org/abs/1703.05175) — few-shot class prototypes.

### Speech robustness and multilingual evaluation

- [SLURP](https://arxiv.org/abs/2011.13205) and [official repository](https://github.com/pswietojanski/slurp) — spoken-language data and ASR baselines.
- [SpokenCSE](https://github.com/MiuLab/SpokenCSE) — ASR-hypothesis derivation and contrastive training for ASR robustness.
- [MASSIVE](https://arxiv.org/abs/2204.08582) and [official repository](https://github.com/alexa/massive) — multilingual intent data.
- [Speech-MASSIVE](https://arxiv.org/abs/2408.03900) and [official repository](https://github.com/hlt-mt/Speech-MASSIVE) — multilingual spoken intent evaluation.
