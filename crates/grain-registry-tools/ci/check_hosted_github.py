"""Read-only public GitHub delivery check of a commit-pinned signed catalogue.

The maintained Rust CLI owns trust, format, expiry and asset validation. This
small operator tool only streams public HTTP bytes against that verified bundle.
No auth, keys, redirects, proxy environment, author code, deployment or retries.
"""
import argparse
from datetime import datetime, timezone
import hashlib
import http.client
import json
import os
from pathlib import Path
import re
import tempfile
import time

from publish_github import git_environment, remote_head, repository_url, run


HOST = "raw.githubusercontent.com"
DOCS = tuple(f"v1/{role}.json{suffix}" for role in ["roots", "index", "revocations"]
             for suffix in ["", ".minisig"])
CHUNK = 64 * 1024
MAX_TOTAL = 1024 * 1024 * 1024
MAX_FILES = 8192 + 4096 * 6 + 8  # Existing Rust committed-capture budget.


def identity(repository, commit, receipt):
    url = repository_url(repository)
    if not re.fullmatch(r"[0-9a-f]{40}", commit):
        raise ValueError("Independent full Git SHA1 commit required")
    if not re.fullmatch(r"[0-9a-f]{64}", receipt):
        raise ValueError("Independent hosting receipt digest required")
    return url


def public_path(name):
    if name in DOCS:
        return name
    if name in ["bundle.json", "current.json"]:
        return f".registry-publication/{name}"
    if re.fullmatch(r"history/[0-9a-f]{64}/(roots|index|revocations)\.json(\.minisig)?", name):
        return f".registry-publication/{name}"
    if re.fullmatch(r"v1/blob/[0-9a-f]{64}\.(grainpack|mcp\.json)", name):
        return name
    if re.fullmatch(r"v1/media/[0-9a-f]{64}\.(md|webp|gif)", name):
        return name
    raise ValueError("Unexpected verified bundle path")


def fingerprint(path):
    digest = hashlib.sha256()
    size = 0
    with path.open("rb") as source:
        while chunk := source.read(CHUNK):
            size += len(chunk)
            if size > MAX_TOTAL:
                raise ValueError("Local file exceeds existing publication budget")
            digest.update(chunk)
    return size, digest.hexdigest()


def inventory(bundle):
    files = []
    total = 0
    # This is a fresh operator-owned Rust capture, not an author checkout.
    for path in sorted(bundle.rglob("*")):
        if path.is_symlink():
            raise ValueError("Linked publication input refused")
        if path.is_dir():
            continue
        if not path.is_file():
            raise ValueError("Publication input must be a regular file")
        name = path.relative_to(bundle).as_posix()
        route = public_path(name)
        size, digest = fingerprint(path)
        total += size
        if not size or total > MAX_TOTAL or len(files) >= MAX_FILES:
            raise ValueError("Publication inventory exceeds existing capture budget")
        files.append({"path": route, "bytes": size, "sha256": digest})
    names = {file["path"] for file in files}
    if not set(DOCS).union({".registry-publication/bundle.json", ".registry-publication/current.json"}) <= names:
        raise ValueError("Incomplete publication inventory")
    return files


def compare_http(repository, ref, file, deadline):
    remaining = min(30, deadline - time.monotonic())
    if remaining <= 0:
        raise TimeoutError("Public delivery deadline ended")
    request_path = f"/{repository}/{ref}/{file['path']}"
    # Fixed HTTPS host and path allowlist. http.client neither follows redirects
    # nor inherits urllib's environment proxy/auth/cookie handlers. Default TLS
    # certificate/hostname verification remains enabled.
    connection = http.client.HTTPSConnection(HOST, timeout=min(10, remaining))
    file_deadline = time.monotonic() + remaining
    try:
        connection.request("GET", request_path, headers={"Accept-Encoding": "identity",
                           "Cache-Control": "no-cache", "User-Agent": "Grain-registry-delivery-check"})
        response = connection.getresponse()
        try:
            if response.status != 200:
                raise ValueError(f"HTTP {response.status}; expected complete 200 response")
            if response.getheader("Content-Encoding", "identity").lower() != "identity":
                raise ValueError("Encoded delivery refused")
            lengths = response.headers.get_all("Content-Length", [])
            if len(lengths) > 1 or (lengths and (not re.fullmatch(r"[0-9]+", lengths[0])
                                               or int(lengths[0]) != file["bytes"])):
                raise ValueError("HTTP length differs from verified file")
            if lengths and response.getheader("Transfer-Encoding"):
                raise ValueError("Ambiguous HTTP framing refused")
            transfer = response.getheader("Transfer-Encoding")
            if transfer and transfer.lower() != "chunked":
                raise ValueError("Unsupported HTTP transfer encoding")
            digest = hashlib.sha256()
            size = 0
            while True:
                if time.monotonic() >= file_deadline:
                    raise TimeoutError("Public file deadline ended")
                # read1 performs at most one buffered/raw read: trickled data
                # cannot hide an unbounded loop inside read(expected_length).
                chunk = response.read1(min(CHUNK, file["bytes"] - size + 1))
                if time.monotonic() >= file_deadline:
                    raise TimeoutError("Public file deadline ended")
                if not chunk:
                    break
                size += len(chunk)
                if size > file["bytes"]:
                    raise ValueError("HTTP response exceeds verified file size")
                digest.update(chunk)
            if size != file["bytes"] or digest.hexdigest() != file["sha256"]:
                raise ValueError("HTTP bytes differ from verified file")
        finally:
            response.close()
    except Exception as error:
        raise RuntimeError(f"Public delivery refused for {ref}/{file['path']}: {error}") from error
    finally:
        connection.close()


def check_delivery(tool, checkout, bundle, repository, commit, receipt, env):
    url = identity(repository, commit, receipt)
    # Capture can read expired authentic history; active hosting must be fresh.
    verify = [tool, "verify-hosting-bundle", "--bundle", bundle, "--expected-receipt-sha256", receipt]
    run(verify, checkout, env)
    roots = json.loads((bundle / "v1/roots.json").read_bytes())
    expected_base = f"https://{HOST}/{repository}/main/v1/"
    if not roots.get("base_urls") or roots["base_urls"][0] != expected_base:
        raise ValueError("Signed primary app host differs from checked GitHub repository")
    files = inventory(bundle)
    if remote_head(checkout, url, env) != commit:
        raise ValueError("Remote main differs from expected publication; no HTTP acceptance granted")
    started = datetime.now(timezone.utc).isoformat()
    deadline = time.monotonic() + 600
    for ref in [commit, "main"]:
        for file in files:
            compare_http(repository, ref, file, deadline)
    # Catch observable drift during the scan, not just a stale initial catalogue.
    for file in files:
        if file["path"] in DOCS:
            compare_http(repository, "main", file, deadline)
    run(verify, checkout, env)  # Freshness must still hold after network time.
    if remote_head(checkout, url, env) != commit:
        raise ValueError("Remote main changed during HTTP check; recheck the reviewed publication")
    return {"schema": 1, "evidence_class": "verified-public-http-delivery-not-app-or-release-approval",
            "status": "Pass", "repository": repository, "commit": commit,
            "bundle_receipt_sha256": receipt, "started_at": started,
            "finished_at": datetime.now(timezone.utc).isoformat(), "files": files,
            "primary_app_base_url": expected_base,
            "requests": 2 * len(files) + len(DOCS), "remote_main_before_and_after": commit}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--tool", required=True, type=Path)
    parser.add_argument("--checkout", required=True, type=Path)
    parser.add_argument("--repository", required=True)
    parser.add_argument("--expected-commit", required=True)
    parser.add_argument("--expected-receipt-sha256", required=True)
    args = parser.parse_args()
    identity(args.repository, args.expected_commit, args.expected_receipt_sha256)
    tool, checkout = args.tool.resolve(strict=True), args.checkout.resolve(strict=True)
    env = git_environment(os.environ)
    with tempfile.TemporaryDirectory(prefix="grain-public-delivery-") as owned:
        capture = Path(owned) / "captured"
        run([tool, "capture-github-publication", "--checkout", checkout, "--repository", args.repository,
             "--expected-commit", args.expected_commit, "--expected-receipt-sha256", args.expected_receipt_sha256,
             "--out", capture], checkout, env)
        print(json.dumps(check_delivery(tool, checkout, capture / "bundle", args.repository,
                                       args.expected_commit, args.expected_receipt_sha256, env), sort_keys=True))


if __name__ == "__main__":
    main()
