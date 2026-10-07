"""Shared Windows/Linux public-CLI boundary checks with real app trust.

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
        unbound = "missing metadata generation binding"
        run("unbound-seed-not-serving-generation", ["verify-serving-tree", "--v1", seed], unbound)
        run("signature-only-verify-route-retired", ["verify", "--v1", seed], unbound)
        store = root / "store"
        run("unbound-seed-cannot-initialize", ["initialize-serving-store", "--v1", seed, "--out", store], unbound)
        run("unbound-seed-cannot-assemble", ["assemble-serving-tree", "--base", seed, "--update", seed, "--out", root / "assembly"], unbound)
        run("unbound-seed-cannot-renew", ["renew-serving-metadata", "--v1", seed, "--expected-snapshot-sha256", "0" * 64,
            "--key", root / "never.key", "--out", root / "renewed"], unbound)
        run("bootstrap-lifetime-refused", ["bootstrap-serving-tree", "--key", root / "never.key", "--expires-days", "0", "--out", root / "bootstrap"], "one through thirty")
        run("bootstrap-explicit-key-required", ["bootstrap-serving-tree", "--key", root / "never.key", "--out", root / "bootstrap"], "Read explicit bootstrap publisher key")
        for name in ["store", "assembly", "renewed", "bootstrap", "never.key"]:
            assert not (root / name).exists(), name
    assert len(commands) == 13
    report = {"tool_sha256": hashlib.sha256(tool.read_bytes()).hexdigest(), "commands": commands,
              "production_keys": 0, "remote_writes": 0}
    (evidence / "report.json").write_text(json.dumps(report, indent=2) + "\n", encoding="utf-8")
    print(json.dumps({"public_cli_checks": len(commands), "production_keys": 0, "remote_writes": 0}))


if __name__ == "__main__":
    main()
