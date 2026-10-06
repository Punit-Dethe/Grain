"""Actual public CLI checkpoint against the pinned legacy repository, no keys.

Uses disposable evidence outside the input checkout; never edits/fetches source,
authenticates, deploys, or launches Grain. Same assertions on Windows and Linux.
"""
import argparse
import hashlib
import json
import os
from pathlib import Path
import subprocess

parser = argparse.ArgumentParser()
parser.add_argument('--tool', type=Path, required=True)
parser.add_argument('--checkout', type=Path, required=True)
parser.add_argument('--output', type=Path, required=True)
args = parser.parse_args()
project = Path(__file__).resolve().parents[3]
fixture = project / 'crates/grain-registry-tools/fixtures/legacy-history-af6e244.json'
# Independent expected bytes, also bound by the checkpoint's source commit pin.
manifest_pin = '6cad006f21b54e7041921dd68db0f8acf30932c0a8617a82a3b671a04a1b56f3'
repository = 'Punit-Dethe/Grain-Extention'
output = args.output.resolve()
checkout = args.checkout.resolve()
tool = args.tool.resolve()
if output == checkout or checkout in output.parents:
    raise ValueError('Evidence output must be outside the source checkout')
output.mkdir(exist_ok=False)
results, git_pids = [], []


def digest(raw):
    return hashlib.sha256(raw).hexdigest()


def child(command, cwd=None, env=None):
    options = {'creationflags': subprocess.CREATE_NO_WINDOW} if os.name == 'nt' else {}
    process = subprocess.Popen([str(a) for a in command], cwd=cwd, env=env,
        stdin=subprocess.DEVNULL, stdout=subprocess.PIPE, stderr=subprocess.PIPE, **options)
    try:
        stdout, stderr = process.communicate(timeout=60)
    except BaseException:
        process.kill()
        process.wait()
        raise
    return process, stdout, stderr


def git(command):
    env = {k: v for k, v in os.environ.items() if not k.upper().startswith('GIT_')}
    env.update(GIT_CONFIG_NOSYSTEM='1', GIT_CONFIG_GLOBAL='NUL' if os.name == 'nt' else '/dev/null', GIT_TERMINAL_PROMPT='0', GIT_NO_LAZY_FETCH='1')
    process, stdout, stderr = child(['git', '--no-replace-objects', '--no-optional-locks'] + command, checkout, env)
    git_pids.append(process.pid)
    assert process.returncode == 0, (command, stderr.decode(errors='replace'))
    return stdout


def run(label, command, error=None):
    process, stdout, stderr = child([tool] + command)
    text = (stdout + stderr).decode('utf-8', errors='replace')
    (output / (label + '.log')).write_text(text, encoding='utf-8')
    if error:
        assert process.returncode != 0 and error in text, (label, text)
    else:
        assert process.returncode == 0, (label, text)
    results.append({'label': label, 'pid': process.pid, 'exit': process.returncode, 'expected': error or 'success'})


raw = fixture.read_bytes()
assert digest(raw) == manifest_pin
manifest = json.loads(raw)
head_before = git(['rev-parse', 'HEAD'])
status_before = git(['status', '--porcelain=v1', '-z', '--untracked-files=normal'])
archive = output / 'archive'
base = ['capture-legacy-history', '--checkout', checkout, '--repository', repository,
    '--manifest', fixture, '--expected-manifest-sha256', manifest_pin, '--out', archive]
run('legacy-actual-complete-signed-history', base)
receipt_raw = (archive / 'legacy-history.json').read_bytes()
receipt = json.loads(receipt_raw)
assert receipt['tip_commit'] == manifest['tip_commit'] and receipt['manifest_sha256'] == manifest_pin
assert [p['commit'] for p in receipt['proofs']] == manifest['commits']
assert len(receipt['proofs']) == 18 and len(receipt['reservations']) == 27
assert len(receipt['conflicted_versions']) == 8 and len(receipt['assets']) == 37
assert 'not-release-approval-no-revocation-proof' in receipt['evidence_class']
assert not (archive / 'v1').exists() and not (archive / 'current.json').exists()
verified_files = 0
for proof in receipt['proofs']:
    for document, sha in proof['documents'].items():
        captured = (archive / 'proofs' / proof['commit'] / document).read_bytes()
        assert captured == git(['cat-file', 'blob', proof['commit'] + ':v1/' + document])
        assert digest(captured) == sha
        verified_files += 1
for name, sha in receipt['assets'].items():
    captured = (archive / 'assets' / name).read_bytes()
    assert digest(captured) == sha
    assert captured == git(['cat-file', 'blob', receipt['asset_sources'][name] + ':v1/' + name])
    verified_files += 1
assert sum(len(p['recovered_assets']) for p in receipt['proofs']) > 0

refused = output / 'refused'
for label, flag, value, error in [
    ('legacy-pin-malformed', '--expected-manifest-sha256', 'bad', 'manifest digest required'),
    ('legacy-pin-mismatch', '--expected-manifest-sha256', '0' * 64, 'independent pin'),
    ('legacy-repository-malformed', '--repository', 'https://elsewhere.invalid/repo', 'OWNER/REPO'),
    ('legacy-origin-mismatch', '--repository', 'elsewhere/registry', 'origin differs'),
    ('legacy-existing-output', '--out', archive, 'already exists'),
    ('legacy-contained-output', '--out', checkout / 'never-created-legacy-output', 'outside'),
]:
    command = base.copy()
    command[command.index('--out') + 1] = refused
    command[command.index(flag) + 1] = value
    run(label, command, error)
    assert not refused.exists()
for label, replacement, error in [
    ('legacy-omitted-proof', {'commits': manifest['commits'][1:]}, 'complete catalogue-changing ancestry'),
    ('legacy-reordered-proof', {'commits': list(reversed(manifest['commits']))}, 'complete catalogue-changing ancestry'),
    ('legacy-duplicate-proof', {'commits': manifest['commits'] + [manifest['commits'][0]]}, 'Malformed'),
    ('legacy-unknown-manifest-authority', {'approval': True}, 'unknown field'),
    ('legacy-missing-tip', {'tip_commit': '0' * 40}, 'Local Git inspection failed'),
]:
    changed = output / (label + '.json')
    changed.write_bytes(json.dumps({**manifest, **replacement}).encode())
    command = base.copy()
    for flag, value in [('--manifest', changed), ('--expected-manifest-sha256', digest(changed.read_bytes())), ('--out', refused)]:
        command[command.index(flag) + 1] = value
    run(label, command, error)
    assert not refused.exists()
assert len(results) == 12
run('migration-cli-default-stack-help', ['--help'])
receipt_pin = digest(receipt_raw)
verify = ['verify-legacy-history', '--archive', archive,
    '--expected-receipt-sha256', receipt_pin]
run('migration-real-archive-reauthenticated', verify)
attempt = verify.copy()
attempt[-1] = '0' * 64
run('migration-archive-pin-mismatch', attempt, 'independent pin')
receipt_file = archive / 'legacy-history.json'
forged = json.dumps({**receipt, 'reservations': []}).encode()
receipt_file.write_bytes(forged)
attempt = verify.copy()
attempt[-1] = digest(forged)
try:
    run('migration-forged-reservations', attempt, 'authenticated history')
finally:
    receipt_file.write_bytes(receipt_raw)
proof_file = archive / 'proofs' / receipt['proofs'][0]['commit'] / 'index.json'
proof_raw = proof_file.read_bytes()
proof_file.write_bytes(proof_raw + b' ')
try:
    run('migration-modified-signed-proof', verify, 'signature does not verify')
finally:
    proof_file.write_bytes(proof_raw)
asset_file = archive / 'assets' / next(iter(receipt['assets']))
asset_raw = asset_file.read_bytes()
asset_file.write_bytes(b'corrupted')
try:
    run('migration-modified-retained-asset', verify, 'bytes differ from signed hash/size')
finally:
    asset_file.write_bytes(asset_raw)
extra = archive / 'unreviewed.json'
extra.write_bytes(b'{}')
try:
    run('migration-unreviewed-archive-file', verify, 'Unexpected')
finally:
    extra.unlink()
sign = ['sign-legacy-migration', '--archive', archive,
    '--expected-receipt-sha256', receipt_pin, '--key', output / 'never-created.key',
    '--expires-days', '30', '--out', refused]
for label, flag, value, error in [
    ('migration-lifetime-refused-before-key', '--expires-days', '0', 'one through thirty'),
    ('migration-contained-output-before-key', '--out', archive / 'never-created', 'outside'),
    ('migration-explicit-protected-key-required', '--expires-days', '30', 'Read protected migration publishing key'),
]:
    attempt = sign.copy()
    attempt[attempt.index(flag) + 1] = value
    run(label, attempt, error)
    assert not refused.exists()
migrate = ['prepare-github-migration', '--checkout', output / 'never-open-checkout',
    '--repository', repository, '--branch', 'main', '--expected-base-commit', '1' * 40,
    '--candidate-commit', '2' * 40, '--legacy-archive', archive,
    '--expected-legacy-receipt-sha256', receipt_pin, '--bundle', output / 'never-open-bundle',
    '--expected-receipt-sha256', '1' * 64, '--out', refused]
for label, flag, value, error in [
    ('migration-handoff-repository-admission', '--repository', 'https://elsewhere.invalid/repo', 'OWNER/REPO'),
    ('migration-handoff-commit-admission', '--candidate-commit', 'bad', 'full Git SHA1'),
    ('migration-handoff-branch-admission', '--branch', 'main:other', 'branch is malformed'),
]:
    attempt = migrate.copy()
    attempt[attempt.index(flag) + 1] = value
    run(label, attempt, error)
    assert not refused.exists()
assert len(results) == 25
assert (archive / 'legacy-history.json').read_bytes() == receipt_raw
assert git(['rev-parse', 'HEAD']) == head_before
assert git(['status', '--porcelain=v1', '-z', '--untracked-files=normal']) == status_before
assert not list(output.rglob('*.key'))
report = {'commands': results, 'git_pids': git_pids, 'manifest_sha256': manifest_pin,
    'receipt_sha256': digest(receipt_raw), 'proofs': len(receipt['proofs']),
    'identity_variants': len(receipt['reservations']), 'conflicts': len(receipt['conflicted_versions']),
    'verified_files': verified_files, 'tool_sha256': digest(tool.read_bytes()),
    'keys_created': 0, 'production_signing_performed': False,
    'hosting_changed': False, 'source_unchanged': True}
(output / 'report.json').write_text(json.dumps(report, indent=2) + '\n', encoding='utf-8')
print(json.dumps({k: v for k, v in report.items() if k not in ['commands', 'git_pids']}))
