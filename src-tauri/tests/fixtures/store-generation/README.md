# Public store-generation fixture

The fixed **public test signer** in `tests/agent-harness/store-fixture.mjs`
signs these files. It is unrelated to the embedded Grain publisher/root keys;
the production verifier must reject its signatures. Only this backend test's
explicit publisher override and the existing guarded acceptance host trust it.

`index.json` binds SHA-256 of the exact embedded `crates/grain-core/seed/roots.json`
and `revocations.json` bytes. The normal revocation fixture copies those seed
JSON bytes but is signed by the public test signer. `mixed-revocations.json` is a
genuinely signed stronger kill switch from another publication; the bound index
must not grant installs with it. Valid negative policy still takes effect.

Generate signatures with `signStoreBytes(Buffer.from(exactBytes))`; never change
JSON formatting after hashing/signing. No production key is read or embedded.
