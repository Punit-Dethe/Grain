"""Product transport boundaries using an owned real HTTP server, no desktop harness.

Only the fixed TLS connection is replaced by loopback HTTP in these tests; the
public CLI has no host/trust override. Rust tests own real signature validation.
"""
import hashlib
import http.client
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
import json
from pathlib import Path
import tempfile
import threading
import time
import unittest
from unittest.mock import patch

import check_hosted_github as delivery


class DeliveryTests(unittest.TestCase):
    def setUp(self):
        self.owned = tempfile.TemporaryDirectory(prefix="grain-delivery-test-")
        self.root = Path(self.owned.name)
        self.bundle = self.root / "bundle"
        (self.bundle / "v1").mkdir(parents=True)
        self.repository = "example/registry"
        self.commit = "1" * 40
        self.receipt = "2" * 64
        self.behavior = None
        self.requests = []
        roots = {"base_urls": [f"https://{delivery.HOST}/{self.repository}/main/v1/"]}
        for name in delivery.DOCS:
            (self.bundle / name).write_bytes(json.dumps(roots).encode() if name == "v1/roots.json" else b"document")
        (self.bundle / "bundle.json").write_bytes(b"receipt")
        (self.bundle / "current.json").write_bytes(b"pointer")
        for name in ["v1/blob/" + "3" * 64 + ".grainpack", "v1/blob/" + "4" * 64 + ".mcp.json",
                     "v1/media/" + "5" * 64 + ".md", "history/" + "6" * 64 + "/index.json"]:
            path = self.bundle / name
            path.parent.mkdir(parents=True, exist_ok=True)
            path.write_bytes(b"data\x00\n")
        self.contents = {file["path"]: self.local_path(file["path"]).read_bytes() for file in delivery.inventory(self.bundle)}
        owner = self
        class Handler(BaseHTTPRequestHandler):
            def log_message(self, *args): pass
            def do_GET(self):
                owner.requests.append((self.path, dict(self.headers)))
                # Retain repository, ref and route validation in the real client.
                route = self.path.split("/", 4)[-1]
                data = owner.contents[route]
                if owner.behavior:
                    return owner.behavior(self, data)
                self.send_response(200)
                self.send_header("Content-Length", str(len(data)))
                self.end_headers()
                self.wfile.write(data)
        self.server = ThreadingHTTPServer(("127.0.0.1", 0), Handler)
        self.thread = threading.Thread(target=self.server.serve_forever, daemon=True)
        self.thread.start()
        self.patch = patch.object(delivery.http.client, "HTTPSConnection",
                                  side_effect=lambda host, timeout: http.client.HTTPConnection(*self.server.server_address, timeout=timeout))
        self.patch.start()

    def tearDown(self):
        self.patch.stop()
        self.server.shutdown()
        self.server.server_close()
        self.thread.join(2)
        self.assertFalse(self.thread.is_alive())
        self.owned.cleanup()

    def local_path(self, public):
        return self.bundle / public.removeprefix(".registry-publication/")

    def check(self):
        return delivery.check_delivery(Path("trusted-tool"), self.root, self.bundle, self.repository,
                                       self.commit, self.receipt, {})

    def compare(self):
        file = {"path": "v1/index.json", "bytes": len(b"document"), "sha256": hashlib.sha256(b"document").hexdigest()}
        delivery.compare_http(self.repository, self.commit, file, time.monotonic() + 5)

    def test_complete_commit_live_and_final_metadata_scan_is_anonymous(self):
        with patch.object(delivery, "run") as verify, patch.object(delivery, "remote_head", return_value=self.commit) as head:
            result = self.check()
        self.assertEqual(result["status"], "Pass")
        self.assertEqual(result["requests"], 2 * len(self.contents) + 6)
        self.assertEqual(len(self.requests), result["requests"])
        self.assertEqual(verify.call_count, 2)
        self.assertTrue(all(call.args[0][1] == "verify-hosting-bundle" for call in verify.call_args_list))
        self.assertEqual(head.call_count, 2)
        for path, headers in self.requests:
            self.assertTrue(path.startswith(f"/{self.repository}/{self.commit}/") or path.startswith(f"/{self.repository}/main/"))
            self.assertNotIn("Authorization", headers)
            self.assertNotIn("Cookie", headers)
            self.assertEqual(headers["Accept-Encoding"], "identity")

    def test_stale_or_mixed_main_is_refused_even_when_commit_matches(self):
        # Change only main's index, not the immutable reference.
        def mixed(handler, data):
            handler.send_response(200); handler.end_headers()
            handler.wfile.write(b"stale!!!" if handler.path.endswith("/main/v1/index.json") else data)
        self.behavior = mixed
        with patch.object(delivery, "run"), patch.object(delivery, "remote_head", return_value=self.commit):
            with self.assertRaisesRegex(RuntimeError, "main/v1/index.json.*bytes differ"):
                self.check()
        self.assertTrue(any(f"/{self.commit}/v1/index.json" in path for path, _ in self.requests))

    def test_redirect_partial_missing_and_encoded_responses_are_not_accepted(self):
        for status, headers, message in [(302, {"Location": "http://127.0.0.1/private"}, "HTTP 302"),
                (206, {}, "HTTP 206"), (404, {}, "HTTP 404"), (200, {"Content-Encoding": "gzip"}, "Encoded")]:
            with self.subTest(status=status, headers=headers):
                def response(handler, data):
                    handler.send_response(status)
                    for key, value in headers.items(): handler.send_header(key, value)
                    handler.end_headers(); handler.wfile.write(data)
                self.behavior = response
                before = len(self.requests)
                with self.assertRaisesRegex(RuntimeError, message): self.compare()
                self.assertEqual(len(self.requests) - before, 1)

    def test_short_extra_wrong_hash_and_length_header_refuse(self):
        for body, headers, message in [(b"short", {}, "bytes differ"), (b"document+", {}, "exceeds"),
                (b"modified", {}, "bytes differ"), (b"document", {"Content-Length": "9"}, "length differs"),
                (b"document", {"Content-Length": "-1"}, "length differs"),
                (b"document", {"Content-Length": "8", "Transfer-Encoding": "chunked"}, "Ambiguous")]:
            with self.subTest(body=body, headers=headers):
                def response(handler, data):
                    handler.send_response(200)
                    for key, value in headers.items(): handler.send_header(key, value)
                    handler.end_headers(); handler.wfile.write(body)
                self.behavior = response
                with self.assertRaisesRegex(RuntimeError, message): self.compare()

    def test_chunked_and_connection_close_bodies_are_measured_not_trusted(self):
        def chunked(handler, data):
            handler.send_response(200); handler.send_header("Transfer-Encoding", "chunked"); handler.end_headers()
            handler.wfile.write(b"8\r\ndocument\r\n0\r\n\r\n")
        self.behavior = chunked
        self.compare()
        self.behavior = lambda handler, data: (handler.send_response(200), handler.end_headers(), handler.wfile.write(data))
        self.compare()

    def test_bad_signature_or_expiry_never_start_http(self):
        for error in ["bad signature", "expired", "missing metadata generation binding"]:
            with patch.object(delivery, "run", side_effect=RuntimeError(error)), patch.object(delivery, "remote_head") as head:
                with self.assertRaisesRegex(RuntimeError, error): self.check()
                head.assert_not_called()
        self.assertEqual(self.requests, [])

    def test_remote_drift_before_and_after_never_grants_acceptance(self):
        with patch.object(delivery, "run"), patch.object(delivery, "remote_head", return_value="7" * 40):
            with self.assertRaisesRegex(ValueError, "differs from expected"): self.check()
        self.assertEqual(self.requests, [])
        with patch.object(delivery, "run"), patch.object(delivery, "remote_head", side_effect=[self.commit, "7" * 40]):
            with self.assertRaisesRegex(ValueError, "changed during"): self.check()

    def test_expiry_after_network_and_wrong_primary_host_refuse(self):
        with patch.object(delivery, "run", side_effect=["", RuntimeError("expired during check")]), patch.object(delivery, "remote_head", return_value=self.commit):
            with self.assertRaisesRegex(RuntimeError, "expired during"): self.check()
        self.requests.clear()
        (self.bundle / "v1/roots.json").write_text(json.dumps({"base_urls": ["https://different.invalid/v1/"]}))
        with patch.object(delivery, "run"), patch.object(delivery, "remote_head") as head:
            with self.assertRaisesRegex(ValueError, "primary app host"): self.check()
            head.assert_not_called()
        self.assertEqual(self.requests, [])

    def test_input_paths_and_deadlines_cannot_redirect_or_start_download(self):
        for name in ["../private", "v1/blob/../index.json", "v1/media/" + "3" * 64 + ".svg", "history/no/index.json", "v1/index.json?url=bad"]:
            with self.assertRaises(ValueError): delivery.public_path(name)
        for slot, value in [(0, "https://evil.invalid/repo"), (0, "example/-repo"), (1, "main"), (2, "bad")]:
            values = [self.repository, self.commit, self.receipt]; values[slot] = value
            with self.assertRaises(ValueError): delivery.identity(*values)
        with self.assertRaises(TimeoutError):
            delivery.compare_http(self.repository, self.commit, {}, time.monotonic() - 1)
        self.assertEqual(self.requests, [])

    def test_ongoing_read_and_transport_failure_have_one_attempt_and_cleanup(self):
        # The real socket still returns bytes. Crossing the monotonic deadline
        # after a read must refuse them rather than certifying a trickled body.
        with patch.object(delivery.time, "monotonic", side_effect=[100, 100, 100, 131]):
            with self.assertRaisesRegex(RuntimeError, "file deadline ended"):
                delivery.compare_http(self.repository, self.commit,
                    {"path": "v1/index.json", "bytes": 8, "sha256": hashlib.sha256(b"document").hexdigest()}, 200)
        self.assertEqual(len(self.requests), 1)
        with patch.object(delivery.http.client, "HTTPSConnection") as factory:
            connection = factory.return_value
            connection.request.side_effect = TimeoutError("connect timed out")
            with self.assertRaisesRegex(RuntimeError, "connect timed out"): self.compare()
            factory.assert_called_once()
            connection.close.assert_called_once()

    def test_unknown_local_file_and_duplicate_length_headers_refuse(self):
        (self.bundle / "unexpected.py").write_text("never executed")
        with self.assertRaisesRegex(ValueError, "Unexpected verified bundle path"):
            delivery.inventory(self.bundle)
        def duplicate(handler, data):
            handler.send_response(200)
            handler.send_header("Content-Length", "8"); handler.send_header("Content-Length", "8")
            handler.end_headers(); handler.wfile.write(data)
        self.behavior = duplicate
        with self.assertRaisesRegex(RuntimeError, "length differs"): self.compare()

    def test_final_catalogue_recheck_catches_late_http_drift(self):
        seen = 0
        def late(handler, data):
            nonlocal seen
            if handler.path.endswith("/main/v1/index.json"): seen += 1
            handler.send_response(200); handler.end_headers()
            handler.wfile.write(b"changed!" if seen == 2 else data)
        self.behavior = late
        with patch.object(delivery, "run"), patch.object(delivery, "remote_head", return_value=self.commit):
            with self.assertRaisesRegex(RuntimeError, "bytes differ"): self.check()


if __name__ == "__main__":
    unittest.main()
