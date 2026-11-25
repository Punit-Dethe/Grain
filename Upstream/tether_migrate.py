#!/usr/bin/env python3
"""Write Grain's upstream policy and decisions in tether's format.

Migration aid for the move to `tether` (see Upstream/FORK-TOOL-PLAN.md, M6).
Until then it is used for golden tests: run it in a throwaway worktree, run
tether there, and compare against the Python tools on the same history.

    python Upstream/tether_migrate.py            # writes .tether/ in the repo root

It reads and never changes the Python tools' inputs; it writes only
`.tether/config.toml` and `.tether/decisions/*.toml`.
"""

from __future__ import annotations

import json
import os
import sys

HERE = os.path.dirname(os.path.abspath(__file__))
ROOT = os.path.dirname(HERE)
sys.path.insert(0, HERE)

import port_audit  # noqa: E402
import ratchet  # noqa: E402
import suppressed_review  # noqa: E402
import sync_upstream  # noqa: E402
from merge_upstream import FROZEN_EXEMPT, FROZEN_PREFIX, UPSTREAM_URL  # noqa: E402


def q(text: str) -> str:
    """A TOML basic string."""
    return json.dumps(text, ensure_ascii=False)


def qlist(items) -> str:
    return "[" + ", ".join(q(item) for item in items) + "]"


def config_toml() -> str:
    out = [
        "[upstream]",
        'remote = "upstream"',
        'branch = "main"',
        'name = "Handy"',
        f"url = {q(UPSTREAM_URL)}",
        f"since = {q(sync_upstream.TRACKING_FLOOR_TS)}",
        "",
        "[[moved]]",
        f"fork = {q(ratchet.HANDY_DIR)}",
        f"upstream = {q(ratchet.HANDY_PREFIX)}",
        f"stay = {qlist(sorted(ratchet.STRAY_ALLOWED))}",
        "",
        "# The frozen frontend moved to src/app/; mapping it lets tether spot",
        "# upstream frontend work that rename detection routes in there.",
        "[[moved]]",
        f"fork = {q(FROZEN_EXEMPT)}",
        f"upstream = {q(FROZEN_PREFIX)}",
        "",
        "[[owned]]",
        f"path = {q(FROZEN_PREFIX)}",
        f"except = {qlist([FROZEN_EXEMPT])}",
        "",
        "[[review]]",
        'name = "frontend"',
        f"paths = {qlist([FROZEN_PREFIX + '*'])}",
        "",
        "[[review]]",
        'name = "suppressed"',
        f"paths = {qlist(suppressed_review.PATTERNS)}",
        "",
        "[budget]",
        f"scope = {qlist([ratchet.SCOPE])}",
        f"exclude = {qlist(sorted(ratchet.EXCLUDE))}",
        "",
        "[port_audit]",
        f"notes_accepted_through = {q(port_audit.STRUCTURED_AUDIT_BASE)}",
        "",
    ]
    for source, entry in sorted(port_audit.load_relocations().items()):
        out += [
            f"[relocations.{q(source)}]",
            f"kind = {q(entry['kind'])}",
            f"fork = {qlist(entry['grain'])}",
            f"why = {q(entry.get('why', ''))}",
            "",
        ]
    return "\n".join(out)


def review_toml(name: str, review: dict) -> list[str]:
    return [
        f"[review.{name}]",
        f"outcome = {q(review['outcome'])}",
        f"problem = {q(review.get('problem', ''))}",
        f"destinations = {qlist(review.get('destinations', []))}",
        f"evidence = {q(review.get('evidence', ''))}",
        "",
    ]


def decision_toml(verdict: dict) -> str:
    out = []
    if verdict.get("status"):
        out.append(f"status = {q(verdict['status'].lower())}")
    if verdict.get("notes"):
        out.append(f"note = {q(verdict['notes'])}")
    if out:
        out.append("")
    for source, port in sorted((verdict.get("ports") or {}).items()):
        out += [
            f"[port.{q(source)}]",
            f"outcome = {q(port['outcome'])}",
            f"destinations = {qlist(port.get('destinations', []))}",
            f"evidence = {q(port.get('evidence', ''))}",
            "",
        ]
    if verdict.get("frontend_review"):
        out += review_toml("frontend", verdict["frontend_review"])
    if verdict.get("suppressed_review"):
        out += review_toml("suppressed", verdict["suppressed_review"])
    return "\n".join(out)


def main() -> int:
    state = os.path.join(ROOT, ".tether")
    decisions = os.path.join(state, "decisions")
    os.makedirs(decisions, exist_ok=True)
    with open(os.path.join(state, "config.toml"), "w", encoding="utf-8", newline="\n") as handle:
        handle.write(config_toml())
    verdicts = sync_upstream.load_verdicts()
    for sha, verdict in verdicts.items():
        body = decision_toml(verdict)
        if body.strip():
            path = os.path.join(decisions, f"{sha}.toml")
            with open(path, "w", encoding="utf-8", newline="\n") as handle:
                handle.write(body)
    print(f"wrote .tether/config.toml and {len(verdicts)} decision(s)")
    return 0


if __name__ == "__main__":
    sys.exit(main())
