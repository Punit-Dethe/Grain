#!/usr/bin/env python3
"""Shared validation for semantic reviews of upstream changes we suppress."""

from __future__ import annotations

import os

OUTCOMES = {"adapted", "already-covered", "not-applicable"}


def validation_failures(review: object, root: str) -> list[str]:
    if not isinstance(review, dict):
        return ["review record is missing"]
    failures: list[str] = []
    outcome = review.get("outcome")
    problem = str(review.get("problem", "")).strip()
    evidence = str(review.get("evidence", "")).strip()
    destinations = review.get("destinations", [])
    if outcome not in OUTCOMES:
        failures.append("invalid outcome")
    if not problem:
        failures.append("underlying problem is missing")
    if not evidence:
        failures.append("verification evidence is missing")
    if not isinstance(destinations, list):
        failures.append("destinations must be a list")
        destinations = []
    if outcome in {"adapted", "already-covered"} and not destinations:
        failures.append(f"{outcome} requires at least one Grain destination")
    for destination in destinations:
        if not isinstance(destination, str) or not destination.strip():
            failures.append("destination is empty")
            continue
        if os.path.isabs(destination):
            failures.append(f"destination must be repository-relative: {destination}")
            continue
        resolved = os.path.abspath(os.path.join(root, destination))
        if os.path.commonpath((os.path.abspath(root), resolved)) != os.path.abspath(root):
            failures.append(f"destination escapes the repository: {destination}")
        elif not os.path.exists(resolved):
            failures.append(f"destination does not exist: {destination}")
    return failures
