# Native companion example

This example proves the Phase 4 Tier-C boundary: an ordinary process connects
through the same authenticated WebSocket as a scripted worker and makes a
capability-checked `storage.set` call. It receives its identity and one-run token
only through Grain's spawn environment.

```powershell
cargo build --release --manifest-path "docs/Extension Platform/examples/native-companion/Cargo.toml"
```

Then enable **Extensions > Developer mode**, choose **Load unpacked**, and select
this folder. Approve `storage` and enable the card. The Developer log shows the
companion start and its host-call response. Disable/unload the extension to stop
the process; Grain also contains it on parent death.

Native projects are intentionally refused by pack import and work only from a
folder explicitly selected in Developer mode until Phase 5 signing exists.
