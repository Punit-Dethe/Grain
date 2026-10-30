//! Provisional extension API profile, independent of package and MCP protocol
//! versions. Numeric exact/caret requirements are the existing native grammar;
//! this is not a general semver range evaluator.

pub const EXTENSION_API_REQUIREMENT: &str = "^1.0";
pub const UNSUPPORTED_EXTENSION_API: &str = "Extension requires an unsupported Grain API profile.";

/// Shared by native author checks, developer loading and installed validation.
/// Missing installed legacy metadata is handled explicitly by the caller.
pub fn api_requirement_supported(requirement: &str, current: &str) -> bool {
    if requirement.len() > 64 {
        return false;
    }
    let requirement = requirement.trim();
    let (caret, version) = match requirement.strip_prefix('^') {
        Some(version) => (true, version),
        None => (false, requirement),
    };
    let Some(required) = numeric_version(version) else {
        return false;
    };
    let Some(current) = numeric_version(current) else {
        return false;
    };
    if caret {
        // The provisional contract is 1.x. Do not invent 0.x caret semantics.
        current.0 > 0 && required.0 == current.0 && current >= required
    } else {
        current == required
    }
}

/// Catalog minimums are literal numeric versions, not ranges. Artifact
/// validation remains mandatory even if the listing minimum is satisfied.
pub fn minimum_api_supported(minimum: &str, current: &str) -> bool {
    if minimum.is_empty() {
        return true;
    } // Existing catalog omitted metadata.
    matches!((numeric_version(minimum), numeric_version(current)), (Some(required), Some(current)) if required <= current)
}

fn numeric_version(version: &str) -> Option<(u64, u64, u64)> {
    if version.len() > 64 {
        return None;
    }
    let mut parts = version.split('.');
    let mut number = || {
        let part = parts.next()?;
        if part.is_empty()
            || !part.bytes().all(|ch| ch.is_ascii_digit())
            || (part.len() > 1 && part.starts_with('0'))
        {
            return None;
        }
        part.parse::<u64>().ok()
    };
    let major = number()?;
    let minor = number()?;
    // Avoid treating a present malformed patch as an omitted patch.
    let patch = match parts.next() {
        None => 0,
        Some(part)
            if !part.is_empty()
                && part.bytes().all(|ch| ch.is_ascii_digit())
                && (part.len() == 1 || !part.starts_with('0')) =>
        {
            part.parse().ok()?
        }
        Some(_) => return None,
    };
    parts.next().is_none().then_some((major, minor, patch))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn canonical_profile_matches_the_existing_host_wire_version() {
        assert_eq!(
            EXTENSION_API_REQUIREMENT,
            format!("^{}", crate::GRAIN_API_VERSION)
        );
    }
    #[test]
    fn existing_exact_and_caret_native_requirements_are_supported() {
        for value in ["^1.0", "^1.0.0", "1.0", "1.0.0", " ^1.0 "] {
            assert!(api_requirement_supported(value, "1.0"), "{value}");
        }
        assert!(api_requirement_supported("^1.0", "1.4"));
        assert!(!api_requirement_supported("1.0", "1.1"));
    }
    #[test]
    fn future_ambiguous_and_unsupported_requirements_are_refused() {
        for value in [
            "",
            " ",
            "^2.0",
            "^1.1",
            "^1.0.1",
            "*",
            ">=1.0",
            "^1",
            "latest",
            "^01.0",
            "+1.0",
            "1.0.",
            "1.0.0.0",
            "1.0-beta",
            "1.0+build",
            "1.０",
            "1.0\n.0",
            "18446744073709551616.0",
            &"1".repeat(65),
        ] {
            assert!(!api_requirement_supported(value, "1.0"), "{value}");
        }
        assert!(!api_requirement_supported("^0.1", "0.9"));
    }
    #[test]
    fn listing_minimums_do_not_authorize_future_or_range_profiles() {
        for value in ["", "0.9", "1.0", "1.0.0"] {
            assert!(minimum_api_supported(value, "1.0"));
        }
        for value in ["1.0.1", "2.0", "^1.0", "*", "1.0-beta", " 1.0 "] {
            assert!(!minimum_api_supported(value, "1.0"));
        }
    }
}
