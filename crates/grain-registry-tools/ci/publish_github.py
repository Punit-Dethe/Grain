"""Manual GitHub activation of an offline-verified, already-signed candidate.

Runs only from the pinned Grain checkout, never from candidate registry code.
No signer, author build, automatic retry, shell evaluation or stored credential.
"""
import argparse
import json
import os
from pathlib import Path
import re
import subprocess
import tempfile


def repository_url(repository):
    if not re.fullmatch(r"[A-Za-z0-9][A-Za-z0-9._-]*/[A-Za-z0-9][A-Za-z0-9._-]*", repository):
        raise ValueError("Expected GitHub OWNER/REPO")
    if any(part.endswith((".", "-")) for part in repository.split("/")) or repository.endswith(".git"):
        raise ValueError("Expected GitHub OWNER/REPO")
    return f"https://github.com/{repository}.git"


def identity(repository, base, candidate, receipt, mode, previous):
    url = repository_url(repository)
    if not all(re.fullmatch(r"[0-9a-f]{40}", value) for value in [base, candidate]) or base == candidate:
        raise ValueError("Distinct full base/candidate commits required")
    if not re.fullmatch(r"[0-9a-f]{64}", receipt):
        raise ValueError("Independent candidate receipt digest required")
    if mode not in ["initial", "update"]:
        raise ValueError("Expected initial or update mode")
    if mode == "update" and not re.fullmatch(r"[0-9a-f]{64}", previous):
        raise ValueError("Independent previous receipt digest required for update")
    if mode == "initial" and previous:
        raise ValueError("Initial publication has no previous receipt")
    return url


def git_environment(source):
    # Keep a token out of verification children; pass it only to final Git calls.
    env = {k: v for k, v in source.items() if not k.startswith("GIT_") and k not in ["GH_TOKEN", "GITHUB_TOKEN", "PUBLICATION_TOKEN"]}
    env.update(GIT_CONFIG_NOSYSTEM="1", GIT_CONFIG_GLOBAL=os.devnull,
               GIT_TERMINAL_PROMPT="0", GIT_NO_LAZY_FETCH="1")
    return env


def run(args, cwd, env):
    # Trusted fixed GitHub endpoint/maintainer CLI only. Reap timed-out children.
    return subprocess.run([str(v) for v in args], cwd=cwd, env=env, check=True,
                          stdin=subprocess.DEVNULL, capture_output=True, text=True, timeout=90).stdout


def push_args(url, base, candidate):
    return ["--no-replace-objects", "-c", "push.followTags=false", "-c",
            "push.recurseSubmodules=no", "push", "--porcelain", "--no-verify",
            f"--force-with-lease=refs/heads/main:{base}", url, f"{candidate}:refs/heads/main"]


def validate_handoff(handoff, repository, base, candidate, receipt, mode, previous):
    if not isinstance(handoff, dict) or type(handoff.get("schema")) is not int:
        raise ValueError("Invalid handoff schema")
    url = identity(repository, base, candidate, receipt, mode, previous)
    expected = {"schema": 1, "repository": repository, "branch": "main",
                "expected_base_commit": base, "candidate_commit": candidate,
                "candidate_receipt_sha256": receipt, "remove_environment_prefix": "GIT_",
                "evidence_class": f"verified-git-{'initial-' if mode == 'initial' else ''}publication-handoff-not-release-approval",
                "push_args": push_args(url, base, candidate),
                "environment": {"GIT_CONFIG_NOSYSTEM": "1", "GIT_CONFIG_GLOBAL": os.devnull,
                                "GIT_TERMINAL_PROMPT": "0", "GIT_NO_LAZY_FETCH": "1"}}
    if mode == "update":
        expected["previous_receipt_sha256"] = previous
    snapshot = handoff.get("snapshot", "")
    if not re.fullmatch(r"[0-9a-f]{64}", snapshot):
        raise ValueError("Invalid verified snapshot")
    expected["snapshot"] = snapshot
    if handoff != expected:
        raise ValueError("Handoff differs from independently supplied publication tuple")
    return url


def remote_head(checkout, url, env):
    rows = run(["git", "--no-replace-objects", "ls-remote", "--exit-code", "--refs",
                url, "refs/heads/main"], checkout, env).strip().splitlines()
    if len(rows) != 1 or not re.fullmatch(r"[0-9a-f]{40}\trefs/heads/main", rows[0]):
        raise ValueError("Ambiguous remote main reference")
    return rows[0].split("\t")[0]


def activate(checkout, handoff, url, base, candidate, env):
    if remote_head(checkout, url, env) != base:
        raise ValueError("Remote main changed; reprepare and review the candidate")
    try:
        run(["git", *handoff["push_args"]], checkout, env)
        if remote_head(checkout, url, env) != candidate:
            raise ValueError("Remote main differs from candidate after push")
    except (subprocess.SubprocessError, ValueError) as error:
        # The push might have succeeded despite a lost response. Never replay it.
        raise RuntimeError("Publication outcome unconfirmed; inspect remote main before any further action. No automatic retry.") from error


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--tool", required=True, type=Path)
    parser.add_argument("--checkout", required=True, type=Path)
    parser.add_argument("--publish", action="store_true", help="Activate only after offline verification")
    parser.add_argument("--check-http", action="store_true", help="After a confirmed push, check anonymous public delivery without retrying publication")
    args = parser.parse_args()
    if args.check_http and not args.publish:
        parser.error("--check-http requires --publish; use check_hosted_github.py for read-only checks")
    values = [os.environ.get(name, "") for name in ["PUBLICATION_REPOSITORY", "PUBLICATION_BASE",
              "PUBLICATION_CANDIDATE", "PUBLICATION_RECEIPT", "PUBLICATION_MODE", "PUBLICATION_PREVIOUS_RECEIPT"]]
    repository, base, candidate, receipt, mode, previous = values
    url = identity(*values)
    tool, checkout = args.tool.resolve(strict=True), args.checkout.resolve(strict=True)
    env = git_environment(os.environ)
    token = os.environ.get("PUBLICATION_TOKEN", "")
    if args.publish and not token:
        raise ValueError("Explicit scoped publication token required")
    with tempfile.TemporaryDirectory(prefix="grain-publication-") as owned:
        scratch = Path(owned)
        bundle = scratch / "candidate"
        run([tool, "capture-github-publication", "--checkout", checkout, "--repository", repository,
             "--expected-commit", candidate, "--expected-receipt-sha256", receipt, "--out", bundle], checkout, env)
        common = ["--checkout", checkout, "--repository", repository, "--branch", "main",
                  "--expected-base-commit", base, "--candidate-commit", candidate,
                  "--bundle", bundle / "bundle", "--expected-receipt-sha256", receipt, "--out", scratch / "handoff"]
        if mode == "update":
            old = scratch / "previous"
            run([tool, "capture-github-publication", "--checkout", checkout, "--repository", repository,
                 "--expected-commit", base, "--expected-receipt-sha256", previous, "--out", old], checkout, env)
            common += ["--previous", old / "bundle", "--previous-receipt-sha256", previous]
        command = "prepare-initial-github-publication" if mode == "initial" else "prepare-github-publication"
        run([tool, command, *common], checkout, env)
        handoff = json.loads((scratch / "handoff/publication.json").read_bytes())
        validate_handoff(handoff, *values)
        print(f"Verified {mode} publication of {candidate}; snapshot {handoff['snapshot']}")
        if not args.publish:
            print("Verification only; remote unchanged")
            return
        if os.name != "posix":
            raise ValueError("Activation requires the Linux publishing runner")
        # Helper contains no token bytes. Its owner-only temp folder is removed
        # on every outcome; the credential stays in final Git child environment.
        helper = scratch / "askpass"
        helper.write_text('#!/bin/sh\ncase "$1" in *Username*) printf "%s\\n" "x-access-token";; *Password*) printf "%s\\n" "$PUBLICATION_TOKEN";; *) exit 1;; esac\n')
        helper.chmod(0o700)
        publish_env = {**env, "GIT_ASKPASS": str(helper), "PUBLICATION_TOKEN": token}
        activate(checkout, handoff, url, base, candidate, publish_env)
        print(f"Remote main confirmed at {candidate}; HTTP/app coherence is a separate acceptance check")
        if args.check_http:
            from check_hosted_github import check_delivery
            try:
                result = check_delivery(tool, checkout, bundle / "bundle", repository, candidate, receipt, env)
            except Exception as error:
                raise RuntimeError(f"Git publication confirmed at {candidate}, but HTTP acceptance failed. Rerun only the read-only check; do not replay publication.") from error
            print(json.dumps(result, sort_keys=True))


if __name__ == "__main__":
    main()
