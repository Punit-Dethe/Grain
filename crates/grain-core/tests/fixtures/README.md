# Fixtures

## `action_routing_golden.json`

A hand-curated set of spoken phrasings labelled with what should happen, built
for the action-routing decision layer that V1 retired (`docs/Extensions V1/
PLAN.md` §9).

**Currently unreferenced, and kept on purpose.** The harness that read it drove
`action_decision`, which no longer exists — but the phrasings in it are real
curation work, and V1-P3 (`grain-ext eval`) needs exactly this material relabelled
one level up: `said` → *which extension*, not which action. The `expect` values
are already qualified `<extension>:<action>`, so the extension label is the part
before the colon.

Two fields in it are dead and should be dropped when it is relabelled:
`domain` on each action (the host taxonomy is retired) and the `choose` /
`escalate` expectations (those were conformal outcomes).

Delete this file and the JSON if P3 builds its golden set from scratch instead.
