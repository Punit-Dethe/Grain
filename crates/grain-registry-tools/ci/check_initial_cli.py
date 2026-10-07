"""Shared Windows/Linux public-CLI checks with real app trust and owned Git.

No trust override, production key, remote write or additional Agent harness.
"""
import argparse
import hashlib
import json
import os
from pathlib import Path
import shutil
import subprocess
import tempfile

from publish_github import git_environment


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--tool", required=True, type=Path)
    parser.add_argument("--evidence", required=True, type=Path)
    args = parser.parse_args()
    tool = args.tool.resolve(strict=True)
    evidence = args.evidence.resolve()
    evidence.mkdir()  # Existing operator output is never overwritten.
    env = git_environment(os.environ)
    commands = []
    def command(label, argv, cwd, error=None):
        result = subprocess.run([str(v) for v in argv], cwd=cwd, env=env,
                                stdin=subprocess.DEVNULL, capture_output=True, text=True, timeout=90)
        output = result.stdout + result.stderr
        (evidence / (label + ".log")).write_text(output, encoding="utf-8")
        if error:
            assert result.returncode != 0 and error in output, (label, output)
        else:
            assert result.returncode == 0, (label, output)
        commands.append({"label": label, "exit": result.returncode, "expected": error or "success"})
        return output
    with tempfile.TemporaryDirectory(prefix="grain-initial-cli-") as owned:
        root = Path(owned)
        run = lambda label, argv, error=None: command(label, [tool, *argv], root, error)
        common = ["prepare-initial-github-publication", "--checkout", root / "absent",
                  "--repository", "example/registry", "--branch", "main", "--expected-base-commit", "1" * 40,
                  "--candidate-commit", "2" * 40, "--bundle", root / "missing-bundle",
                  "--expected-receipt-sha256", "3" * 64, "--out", root / "refused"]
        help_text = run("initial-public-cli-help", [common[0], "--help"])
        assert "--previous" not in help_text
        for label, flag, value, error in [
            ("initial-repository-refused", "--repository", "https://evil.invalid/repo", "OWNER/REPO"),
            ("initial-candidate-refused", "--candidate-commit", "main", "full Git SHA1"),
            ("initial-identical-commits-refused", "--candidate-commit", "1" * 40, "distinct full Git SHA1"),
            ("initial-branch-refused", "--branch", "main:other", "branch is malformed")]:
            attempt = common.copy(); attempt[attempt.index(flag) + 1] = value
            run(label, attempt, error)
        run("initial-fabricated-previous-option-refused", [*common, "--previous", root / "fake"], "unexpected argument")
        assert not (root / "refused").exists() and not (root / "absent").exists()
        seed = root / "seed"; seed.mkdir()
        for folder in ["blob", "media"]: (seed / folder).mkdir()
        source = Path(__file__).resolve().parents[2] / "grain-core/seed"
        for name in ["roots", "index", "revocations"]:
            for suffix in [".json", ".json.minisig"]: shutil.copyfile(source / (name + suffix), seed / (name + suffix))
        run("initial-current-app-trust-chain", ["verify-serving-tree", "--v1", seed])
        store = root / "store"
        run("initial-current-seed-store", ["initialize-serving-store", "--v1", seed, "--out", store])
        pointer = hashlib.sha256((store / "current.json").read_bytes()).hexdigest()
        bundle = root / "bundle"
        run("initial-current-seed-hosting", ["export-hosting-bundle", "--store", store,
            "--expected-current-sha256", pointer, "--out", bundle])
        pin = hashlib.sha256((bundle / "bundle.json").read_bytes()).hexdigest()
        repo = root / "repo"; repo.mkdir()
        def git(argv):
            result = subprocess.run(["git", *[str(v) for v in argv]], cwd=repo, env=env,
                                    capture_output=True, text=True, check=True, timeout=30)
            return result.stdout.strip()
        git(["init", "--initial-branch=main"])
        for key, value in [("user.name", "Fixture"), ("user.email", "fixture@example.invalid"),
                           ("commit.gpgSign", "false"), ("core.autocrlf", "false"),
                           ("remote.origin.url", "https://github.com/example/registry.git")]:
            git(["config", "--local", key, value])
        (repo / "README.md").write_text("preserve unrelated source")
        git(["add", "."]); git(["commit", "-m", "base"]); base = git(["rev-parse", "HEAD"])
        shutil.copytree(bundle / "v1", repo / "v1")
        proof = repo / ".registry-publication"; proof.mkdir()
        for name in ["bundle.json", "current.json"]: shutil.copyfile(bundle / name, proof / name)
        git(["add", "."]); git(["commit", "-m", "seed candidate"]); candidate = git(["rev-parse", "HEAD"])
        capture = root / "capture"
        run("initial-committed-app-trust-capture", ["capture-github-publication", "--checkout", repo,
            "--repository", "example/registry", "--expected-commit", candidate,
            "--expected-receipt-sha256", pin, "--out", capture])
        gate = [common[0], "--checkout", repo, "--repository", "example/registry", "--branch", "main",
                "--expected-base-commit", base, "--candidate-commit", candidate, "--bundle", capture / "bundle",
                "--expected-receipt-sha256", pin, "--out", root / "refused"]
        run("initial-seed-cannot-be-published-with-bootstrap-exemption", gate, "thirty days")
        runner_env = {**env, "PUBLICATION_REPOSITORY": "example/registry", "PUBLICATION_BASE": base,
                      "PUBLICATION_CANDIDATE": candidate, "PUBLICATION_RECEIPT": pin,
                      "PUBLICATION_MODE": "initial", "PUBLICATION_PREVIOUS_RECEIPT": ""}
        result = subprocess.run([os.sys.executable, str(Path(__file__).with_name("publish_github.py")),
                                "--tool", str(tool), "--checkout", str(repo)], cwd=root, env=runner_env,
                                stdin=subprocess.DEVNULL, capture_output=True, text=True, timeout=90)
        assert result.returncode != 0 and "prepare-initial-github-publication" in result.stderr
        commands.append({"label": "initial-publication-runner-refuses-seed-without-network-or-write", "exit": result.returncode, "expected": "verified gate refusal"})
        (evidence / "initial-runner-refusal.log").write_text(result.stdout + result.stderr, encoding="utf-8")
        assert git(["rev-parse", "HEAD"]) == candidate and not (root / "refused").exists()
    assert len(commands) == 12
    report = {"tool_sha256": hashlib.sha256(tool.read_bytes()).hexdigest(), "commands": commands,
              "production_keys": 0, "remote_writes": 0}
    (evidence / "report.json").write_text(json.dumps(report, indent=2) + "\n", encoding="utf-8")
    print(json.dumps({"public_cli_checks": len(commands), "production_keys": 0, "remote_writes": 0}))


if __name__ == "__main__":
    main()
