"""Focused publisher boundary tests; real Git races, no GitHub/token/signing key."""
import os
from pathlib import Path
import subprocess
import tempfile
import unittest
from unittest.mock import patch

import publish_github as publisher


class PublisherTests(unittest.TestCase):
    def test_inputs_and_handoff_cannot_redirect_or_add_git_flags(self):
        values = ["example/registry", "1" * 40, "2" * 40, "3" * 64, "initial", ""]
        for slot, value in [(0, "https://evil.invalid/repo"), (1, "main"), (2, "1" * 40),
                            (3, "bad"), (4, "other"), (5, "4" * 64)]:
            attempt = values.copy(); attempt[slot] = value
            with self.assertRaises(ValueError): publisher.identity(*attempt)
        url = publisher.identity(*values)
        handoff = {"schema": 1, "evidence_class": "verified-git-initial-publication-handoff-not-release-approval",
                   "repository": values[0], "branch": "main", "expected_base_commit": values[1],
                   "candidate_commit": values[2], "candidate_receipt_sha256": values[3],
                   "snapshot": "5" * 64, "push_args": publisher.push_args(url, *values[1:3]),
                   "remove_environment_prefix": "GIT_", "environment": publisher.git_environment({})}
        self.assertEqual(publisher.validate_handoff(handoff, *values), url)
        for field, value in [("push_args", ["push", "--force", url]), ("branch", "other"),
                             ("previous_receipt_sha256", "4" * 64), ("snapshot", "bad"), ("schema", 2), ("schema", True)]:
            with self.assertRaises(ValueError): publisher.validate_handoff({**handoff, field: value}, *values)

    def test_verification_drops_tokens_and_git_config_injection(self):
        env = publisher.git_environment({"PATH": os.environ["PATH"], "GIT_CONFIG_COUNT": "1",
              "GIT_CONFIG_KEY_0": "url.evil.insteadOf", "GIT_ASKPASS": "bad",
              "PUBLICATION_TOKEN": "secret", "GH_TOKEN": "secret", "GITHUB_TOKEN": "secret"})
        self.assertEqual(env, {"PATH": os.environ["PATH"], **publisher.git_environment({})})

    def test_actual_conditional_push_and_both_competing_writer_windows(self):
        with tempfile.TemporaryDirectory(prefix="grain-publisher-test-") as owned:
            root = Path(owned); repo = root / "repo"; remote = root / "remote.git"
            repo.mkdir(); env = publisher.git_environment(os.environ)
            def git(args, cwd=repo): return publisher.run(["git", *args], cwd, env).strip()
            git(["init", "--initial-branch=main"])
            for key, value in [("user.name", "Fixture"), ("user.email", "fixture@example.invalid"),
                               ("commit.gpgSign", "false"), ("core.autocrlf", "false")]:
                git(["config", "--local", key, value])
            (repo / "document").write_text("base"); git(["add", "."]); git(["commit", "-m", "base"])
            base = git(["rev-parse", "HEAD"])
            (repo / "document").write_text("candidate"); git(["add", "."]); git(["commit", "-m", "candidate"])
            candidate = git(["rev-parse", "HEAD"])
            (repo / "document").write_text("competitor"); git(["add", "."]); git(["commit", "-m", "competitor"])
            winner = git(["rev-parse", "HEAD"])
            git(["init", "--bare", remote], root); git(["push", remote, f"{base}:refs/heads/main"])
            url = "https://github.com/example/registry.git"
            handoff = {"push_args": publisher.push_args(url, base, candidate)}
            actual_run = publisher.run; calls = []
            def transport(args, cwd, child_env):
                # Substitute only the fixed GitHub endpoint for this owned bare
                # remote. All real Git flags, leases and objects remain intact.
                calls.append(list(args))
                return actual_run([str(remote) if arg == url else arg for arg in args], cwd, child_env)
            with patch.object(publisher, "run", side_effect=transport):
                publisher.activate(repo, handoff, url, base, candidate, env)
                self.assertEqual(git(["rev-parse", "refs/heads/main"], remote), candidate)
                calls.clear()
                with self.assertRaisesRegex(ValueError, "Remote main changed"):
                    publisher.activate(repo, handoff, url, base, candidate, env)
                self.assertFalse(any("push" in args for args in calls))
            git(["push", remote, f"{winner}:refs/heads/main"])
            git(["update-ref", "refs/heads/main", base], remote)
            calls.clear()
            def race(args, cwd, child_env):
                if "push" in args: git(["update-ref", "refs/heads/main", winner], remote)
                return transport(args, cwd, child_env)
            with patch.object(publisher, "run", side_effect=race):
                with self.assertRaisesRegex(RuntimeError, "No automatic retry"):
                    publisher.activate(repo, handoff, url, base, candidate, env)
            self.assertEqual(git(["rev-parse", "refs/heads/main"], remote), winner)
            self.assertEqual(sum("push" in args for args in calls), 1)

    def test_uncertain_response_and_postcondition_never_repeat_push(self):
        base, candidate = "1" * 40, "2" * 40
        url = "https://github.com/example/registry.git"
        handoff = {"push_args": publisher.push_args(url, base, candidate)}
        for failure in [subprocess.TimeoutExpired("git", 90), f"{'3' * 40}\trefs/heads/main\n"]:
            results = [f"{base}\trefs/heads/main\n", failure] if isinstance(failure, Exception) else [f"{base}\trefs/heads/main\n", "pushed", failure]
            with patch.object(publisher, "run", side_effect=results) as run:
                with self.assertRaisesRegex(RuntimeError, "No automatic retry"):
                    publisher.activate(Path("."), handoff, url, base, candidate, {})
                self.assertEqual(sum("push" in call.args[0] for call in run.call_args_list), 1)


if __name__ == "__main__":
    unittest.main()
