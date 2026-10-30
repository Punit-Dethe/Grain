"""Disposable public-CLI signing credential checks; no production key or network.

Run against an explicit built binary. Uses the existing checkpoint evidence
format, not an Agent/UI fixture or an alternative signing implementation.
"""
import argparse
import hashlib
import json
import os
from pathlib import Path
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
    evidence.mkdir()  # Never replace an earlier acceptance record.
    env = git_environment(os.environ)
    commands = []
    with tempfile.TemporaryDirectory(prefix="grain-key-cli-") as owned:
        root = Path(owned)
        password = "disposable checkpoint password; never production"
        password_file = root / "password"
        password_file.write_text(password + "\n", encoding="utf-8")
        password_file.chmod(0o600)

        def run(label, argv, error=None):
            result = subprocess.run([str(tool), *map(str, argv)], cwd=root, env=env,
                                    stdin=subprocess.DEVNULL, capture_output=True,
                                    text=True, timeout=120)
            output = result.stdout + result.stderr
            assert password not in output, label
            (evidence / (label + ".log")).write_text(output, encoding="utf-8")
            if error:
                assert result.returncode != 0 and error in output, (label, output)
            else:
                assert result.returncode == 0, (label, output)
            commands.append({"label": label, "exit": result.returncode,
                             "expected": error or "success"})
            return output

        generate = ["keygen", "--out", root / "keys", "--name", "publisher"]
        run("keygen-missing-unlock-refused", generate, "Explicit key unlock required")
        assert not (root / "keys").exists()
        run("keygen-piped-prompt-refused", [*generate, "--prompt-key-password"],
            "requires an attached terminal")
        assert not (root / "keys").exists()
        run("keygen-conflicting-unlock-refused",
            [*generate, "--key-password-file", password_file, "--dev-empty-key-password"],
            "cannot be used with")
        assert not (root / "keys").exists()
        run("keygen-path-name-refused",
            ["keygen", "--out", root / "keys", "--name", "../escape",
             "--key-password-file", password_file], "Key name must contain")
        assert not (root / "keys").exists()
        run("keygen-protected", [*generate, "--key-password-file", password_file])
        key = root / "keys/publisher.key"
        before = key.read_bytes()
        run("keygen-existing-pair-refused-before-unlock", generate,
            "Key output already exists")
        assert key.read_bytes() == before
        doc = root / "document.json"
        doc.write_text('{"disposable":true}\n', encoding="utf-8")
        signature = root / "document.minisig"
        sign = ["sign", "--key", key, "--input", doc, "--out", signature]
        run("sign-missing-unlock-refused", sign, "Explicit key unlock required")
        assert not signature.exists()
        password_file.write_text("wrong disposable password", encoding="utf-8")
        run("sign-wrong-password-refused", [*sign, "--key-password-file", password_file],
            "Unlock signing key")
        assert not signature.exists()
        password_file.write_text("\n", encoding="utf-8")
        run("sign-empty-password-refused", [*sign, "--key-password-file", password_file],
            "must be one nonempty")
        assert not signature.exists()
        password_file.write_bytes((password + "\r\n").encode("utf-8"))
        run("sign-protected", [*sign, "--key-password-file", password_file])
        assert signature.stat().st_size > 0 and key.read_bytes() == before
        run("verification-does-not-accept-credentials",
            ["publine", "--pubkey", root / "keys/publisher.pub",
             "--key-password-file", password_file], "Credential options apply only")
        run("keygen-explicit-development", ["keygen", "--out", root / "development",
                                            "--name", "disposable", "--dev-empty-key-password"])
    assert len(commands) == 12
    report = {"tool_sha256": hashlib.sha256(tool.read_bytes()).hexdigest(),
              "commands": commands, "production_keys": 0, "remote_writes": 0}
    (evidence / "report.json").write_text(json.dumps(report, indent=2) + "\n", encoding="utf-8")
    print(json.dumps({"key_cli_checks": len(commands), "production_keys": 0, "remote_writes": 0}))


if __name__ == "__main__":
    main()
