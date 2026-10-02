"""Generate a disposable localhost certificate; never modifies OS trust stores."""
import sys
import ipaddress
from pathlib import Path
from datetime import datetime, timedelta, timezone
from cryptography import x509
from cryptography.hazmat.primitives import hashes, serialization
from cryptography.hazmat.primitives.asymmetric import rsa
from cryptography.x509.oid import NameOID, ExtendedKeyUsageOID

root = Path(sys.argv[1]).resolve(strict=True)
assert (root / '.grain-agent-harness.json').is_file()
purpose = sys.argv[2] if len(sys.argv) > 2 else 'native'
assert purpose in ('native', 'mcp')
out = root / ('mcp-tls' if purpose == 'mcp' else 'auth-tls')
out.mkdir()
assert out.resolve().parent == root
key = rsa.generate_private_key(public_exponent=65537, key_size=2048)
name = x509.Name([x509.NameAttribute(NameOID.COMMON_NAME, 'Grain disposable OAuth fixture')])
now = datetime.now(timezone.utc)
cert = (x509.CertificateBuilder().subject_name(name).issuer_name(name)
        .public_key(key.public_key()).serial_number(x509.random_serial_number())
        .not_valid_before(now - timedelta(minutes=5)).not_valid_after(now + timedelta(days=2))
        .add_extension(x509.SubjectAlternativeName([x509.IPAddress(ipaddress.ip_address('127.0.0.1'))]), critical=False)
        .add_extension(x509.BasicConstraints(ca=True, path_length=0), critical=True)
        .add_extension(x509.ExtendedKeyUsage([ExtendedKeyUsageOID.SERVER_AUTH]), critical=False)
        .add_extension(x509.KeyUsage(digital_signature=True, content_commitment=False,
                                   key_encipherment=True, data_encipherment=False, key_agreement=False,
                                   key_cert_sign=True, crl_sign=False, encipher_only=None, decipher_only=None), critical=True)
        .sign(key, hashes.SHA256()))
if purpose == 'mcp':
    # reqwest 0.13's root-only verifier requires an actual leaf certificate.
    # The CA private key is never written; neither trust store is modified.
    (out / 'ca.pem').write_bytes(cert.public_bytes(serialization.Encoding.PEM))
    leaf_key = rsa.generate_private_key(public_exponent=65537, key_size=2048)
    leaf_name = x509.Name([x509.NameAttribute(NameOID.COMMON_NAME, 'Grain disposable MCP fixture')])
    cert = (x509.CertificateBuilder().subject_name(leaf_name).issuer_name(name)
            .public_key(leaf_key.public_key()).serial_number(x509.random_serial_number())
            .not_valid_before(now - timedelta(minutes=5)).not_valid_after(now + timedelta(days=2))
            .add_extension(x509.SubjectAlternativeName([x509.IPAddress(ipaddress.ip_address('127.0.0.1'))]), critical=False)
            .add_extension(x509.BasicConstraints(ca=False, path_length=None), critical=True)
            .add_extension(x509.ExtendedKeyUsage([ExtendedKeyUsageOID.SERVER_AUTH]), critical=False)
            .add_extension(x509.KeyUsage(digital_signature=True, content_commitment=False,
                                       key_encipherment=True, data_encipherment=False, key_agreement=False,
                                       key_cert_sign=False, crl_sign=False, encipher_only=None, decipher_only=None), critical=True)
            .sign(key, hashes.SHA256()))
    key = leaf_key
(out / 'key.pem').write_bytes(key.private_bytes(serialization.Encoding.PEM,
                              serialization.PrivateFormat.PKCS8, serialization.NoEncryption()))
(out / 'cert.pem').write_bytes(cert.public_bytes(serialization.Encoding.PEM))
