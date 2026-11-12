//! Stores values larger than one OS credential across several entries.
//!
//! Windows Credential Manager caps a credential at 2,560 bytes, and some
//! providers issue longer tokens (monday.com). A value that fits stays in one
//! entry. A larger one is split into parts named after a fresh generation; the
//! main entry gets a small manifest, written last. A crash mid-write therefore
//! leaves the previous value readable, and old parts are removed only after the
//! new manifest is in place.

pub(super) const PART_BYTES: usize = 2048;
const MAX_TOTAL: usize = 128 * 1024;
const MAX_PARTS: usize = MAX_TOTAL / PART_BYTES;
const PREFIX: &str = "grain-vault-parts:1:";

/// One credential service. `delete` treats a missing entry as success.
pub(super) trait Io {
    fn get(&mut self, account: &str) -> Result<Option<Vec<u8>>, String>;
    fn set(&mut self, account: &str, value: &[u8]) -> Result<(), String>;
    fn delete(&mut self, account: &str) -> Result<(), String>;
}

struct Manifest {
    generation: String,
    parts: usize,
    len: usize,
}

fn part_account(account: &str, generation: &str, index: usize) -> String {
    format!("{account}#part.{generation}.{index}")
}

fn manifest_bytes(manifest: &Manifest) -> Vec<u8> {
    format!(
        "{PREFIX}{}:{}:{}",
        manifest.generation, manifest.parts, manifest.len
    )
    .into_bytes()
}

fn parse_manifest(bytes: &[u8]) -> Option<Manifest> {
    let text = std::str::from_utf8(bytes).ok()?.strip_prefix(PREFIX)?;
    let mut fields = text.split(':');
    let generation = fields.next()?;
    let parts: usize = fields.next()?.parse().ok()?;
    let len: usize = fields.next()?.parse().ok()?;
    if fields.next().is_some()
        || !valid_generation(generation)
        || !(2..=MAX_PARTS).contains(&parts)
        || len > MAX_TOTAL
        || len <= (parts - 1) * PART_BYTES
        || len > parts * PART_BYTES
    {
        return None;
    }
    Some(Manifest {
        generation: generation.into(),
        parts,
        len,
    })
}

fn valid_generation(generation: &str) -> bool {
    !generation.is_empty()
        && generation.len() <= 32
        && generation.bytes().all(|byte| byte.is_ascii_hexdigit())
}

/// Best-effort wipe of secret bytes that are about to be dropped.
fn wipe(bytes: &mut [u8]) {
    for byte in bytes.iter_mut() {
        // SAFETY: `byte` is a valid, exclusive reference into the slice.
        unsafe { std::ptr::write_volatile(byte, 0) };
    }
}

fn stored_manifest(io: &mut impl Io, account: &str) -> Result<Option<Manifest>, String> {
    Ok(io.get(account)?.and_then(|mut bytes| {
        let manifest = parse_manifest(&bytes);
        wipe(&mut bytes);
        manifest
    }))
}

pub(super) fn read(io: &mut impl Io, account: &str) -> Result<Option<Vec<u8>>, String> {
    let Some(mut primary) = io.get(account)? else {
        return Ok(None);
    };
    let Some(manifest) = parse_manifest(&primary) else {
        return Ok(Some(primary));
    };
    wipe(&mut primary);
    let mut value = Vec::with_capacity(manifest.len);
    for index in 0..manifest.parts {
        match io.get(&part_account(account, &manifest.generation, index)) {
            Ok(Some(mut part)) => {
                value.extend_from_slice(&part);
                wipe(&mut part);
            }
            Ok(None) | Err(_) => {
                wipe(&mut value);
                return Err("stored credential is incomplete".into());
            }
        }
    }
    if value.len() != manifest.len {
        wipe(&mut value);
        return Err("stored credential is incomplete".into());
    }
    Ok(Some(value))
}

/// `generation` must be fresh hex (≤ 32 chars) for every write.
pub(super) fn write(
    io: &mut impl Io,
    account: &str,
    value: &[u8],
    generation: &str,
) -> Result<(), String> {
    if value.len() > MAX_TOTAL {
        return Err("credential is too large".into());
    }
    if !valid_generation(generation) {
        return Err("invalid credential generation".into());
    }
    let previous = stored_manifest(io, account)?;
    if value.len() <= PART_BYTES {
        io.set(account, value)?;
    } else {
        if previous
            .as_ref()
            .is_some_and(|old| old.generation == generation)
        {
            return Err("credential generation collided; retry".into());
        }
        let manifest = Manifest {
            generation: generation.into(),
            parts: value.len().div_ceil(PART_BYTES),
            len: value.len(),
        };
        for (index, chunk) in value.chunks(PART_BYTES).enumerate() {
            if let Err(error) = io.set(&part_account(account, generation, index), chunk) {
                discard_parts(io, account, generation, index + 1);
                return Err(error);
            }
        }
        if let Err(error) = io.set(account, &manifest_bytes(&manifest)) {
            discard_parts(io, account, generation, manifest.parts);
            return Err(error);
        }
    }
    // The new value is live; parts of the replaced one are now unreachable.
    if let Some(old) = previous {
        discard_parts(io, account, &old.generation, old.parts);
    }
    Ok(())
}

fn discard_parts(io: &mut impl Io, account: &str, generation: &str, count: usize) {
    for index in 0..count {
        let _ = io.delete(&part_account(account, generation, index));
    }
}

/// Parts go first: if one cannot be removed, the manifest still names it and a
/// retry can finish the job.
pub(super) fn delete(io: &mut impl Io, account: &str) -> Result<(), String> {
    // An unreadable main entry has no known parts; still remove the entry.
    if let Ok(Some(manifest)) = stored_manifest(io, account) {
        for index in 0..manifest.parts {
            io.delete(&part_account(account, &manifest.generation, index))?;
        }
    }
    io.delete(account)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;

    /// In-memory vault with the Windows per-entry size limit.
    #[derive(Default)]
    struct Vault {
        entries: BTreeMap<String, Vec<u8>>,
        fail_set_on: Option<String>,
    }

    impl Io for Vault {
        fn get(&mut self, account: &str) -> Result<Option<Vec<u8>>, String> {
            Ok(self.entries.get(account).cloned())
        }
        fn set(&mut self, account: &str, value: &[u8]) -> Result<(), String> {
            if value.len() > 2560 {
                return Err("longer than the platform limit".into());
            }
            if self.fail_set_on.as_deref() == Some(account) {
                return Err("write failed".into());
            }
            self.entries.insert(account.into(), value.to_vec());
            Ok(())
        }
        fn delete(&mut self, account: &str) -> Result<(), String> {
            self.entries.remove(account);
            Ok(())
        }
    }

    fn token(len: usize) -> Vec<u8> {
        (0..len).map(|i| b'a' + (i % 26) as u8).collect()
    }

    #[test]
    fn small_values_stay_in_one_entry() {
        let mut vault = Vault::default();
        write(&mut vault, "linear", b"{\"t\":1}", "1").unwrap();
        assert_eq!(vault.entries.len(), 1);
        assert_eq!(read(&mut vault, "linear").unwrap().unwrap(), b"{\"t\":1}");
    }

    #[test]
    fn long_tokens_round_trip_past_the_windows_limit() {
        let mut vault = Vault::default();
        let value = token(6000);
        write(&mut vault, "monday", &value, "a1").unwrap();
        assert_eq!(vault.entries.len(), 1 + 3);
        assert!(vault.entries.values().all(|entry| entry.len() <= 2560));
        assert_eq!(read(&mut vault, "monday").unwrap().unwrap(), value);
        assert_eq!(read(&mut vault, "missing").unwrap(), None);
    }

    #[test]
    fn rewrites_remove_the_replaced_parts() {
        let mut vault = Vault::default();
        write(&mut vault, "monday", &token(6000), "a1").unwrap();
        write(&mut vault, "monday", &token(4500), "a2").unwrap();
        assert_eq!(vault.entries.len(), 1 + 3);
        assert!(!vault.entries.keys().any(|key| key.contains(".a1.")));
        write(&mut vault, "monday", b"{}", "a3").unwrap();
        assert_eq!(vault.entries.len(), 1);
        assert_eq!(read(&mut vault, "monday").unwrap().unwrap(), b"{}");
    }

    #[test]
    fn a_failed_write_keeps_the_previous_value() {
        let mut vault = Vault::default();
        let old = token(5000);
        write(&mut vault, "monday", &old, "a1").unwrap();
        vault.fail_set_on = Some("monday".into());
        assert!(write(&mut vault, "monday", &token(7000), "a2").is_err());
        assert_eq!(read(&mut vault, "monday").unwrap().unwrap(), old);
        assert!(!vault.entries.keys().any(|key| key.contains(".a2.")));
        vault.fail_set_on = Some("monday#part.a3.1".into());
        assert!(write(&mut vault, "monday", &token(7000), "a3").is_err());
        assert_eq!(read(&mut vault, "monday").unwrap().unwrap(), old);
        assert!(!vault.entries.keys().any(|key| key.contains(".a3.")));
    }

    #[test]
    fn delete_removes_every_part() {
        let mut vault = Vault::default();
        write(&mut vault, "monday", &token(6000), "a1").unwrap();
        write(&mut vault, "notion", b"{}", "b1").unwrap();
        delete(&mut vault, "monday").unwrap();
        assert_eq!(vault.entries.keys().collect::<Vec<_>>(), ["notion"]);
        delete(&mut vault, "monday").unwrap();
    }

    #[test]
    fn missing_parts_or_bad_manifests_are_never_returned_as_credentials() {
        let mut vault = Vault::default();
        write(&mut vault, "monday", &token(6000), "a1").unwrap();
        vault.entries.remove("monday#part.a1.1");
        assert!(read(&mut vault, "monday").is_err());
        for bad in [
            "grain-vault-parts:1:a1:1:100",
            "grain-vault-parts:1:zz:2:3000",
            "grain-vault-parts:1:a1:2:5000",
            "grain-vault-parts:1:a1:2:3000:extra",
            "grain-vault-parts:1::2:3000",
        ] {
            assert!(parse_manifest(bad.as_bytes()).is_none(), "{bad}");
        }
        assert!(parse_manifest(b"grain-vault-parts:1:a1:2:3000").is_some());
        assert!(write(&mut vault, "x", &token(MAX_TOTAL + 1), "c1").is_err());
        assert!(write(&mut vault, "x", b"{}", "not-hex").is_err());
    }
}
