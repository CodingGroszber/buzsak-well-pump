//! Pure decision logic of the OTA update path: version policy, credential comparison, hex
//! formatting and the software rollback counter. No I/O, so all of it is unit tested on the host.

/// A parsed `major.minor.patch` version; tuples compare numerically field by field.
pub type Version = (u32, u32, u32);

/// Parses `major.minor.patch`, ignoring a leading `v` and any `-pre` / `+build` suffix.
///
/// # Arguments
/// * `v` - The version text, e.g. `"0.1.1"`, `"v1.2.3"` or `"1.2.3-rc1+abc"`.
///
/// # Returns
/// The numeric version, or `None` when the text is not exactly three dot-separated numbers.
pub fn parse_version(v: &str) -> Option<Version> {
    let core = v.trim_start_matches('v').split(['-', '+']).next()?;
    let mut parts = core.split('.');
    let parsed = (
        parts.next()?.parse().ok()?,
        parts.next()?.parse().ok()?,
        parts.next()?.parse().ok()?,
    );
    parts.next().is_none().then_some(parsed)
}

/// Compares two byte strings without stopping at the first difference, so the comparison time
/// does not reveal how much of a credential matched.
///
/// # Arguments
/// * `a` - First byte string.
/// * `b` - Second byte string.
///
/// # Returns
/// `true` when both have the same length and content.
pub fn constant_time_eq(a: &[u8], b: &[u8]) -> bool {
    a.len() == b.len() && a.iter().zip(b).fold(0u8, |acc, (x, y)| acc | (x ^ y)) == 0
}

/// Tells whether a version carries a pre-release suffix such as `0.1.1-test`.
///
/// Test and simulator builds carry such a suffix; production firmware refuses them.
///
/// # Arguments
/// * `v` - The version text.
///
/// # Returns
/// `true` when the version (ignoring a `+build` part) contains a `-suffix`.
pub fn is_prerelease(v: &str) -> bool {
    v.trim_start_matches('v')
        .split('+')
        .next()
        .is_some_and(|core| core.contains('-'))
}

/// Formats bytes as lowercase hexadecimal.
///
/// # Arguments
/// * `bytes` - The bytes to format.
///
/// # Returns
/// Two lowercase hex digits per byte, e.g. `[0x0f, 0xab]` becomes `"0fab"`.
pub fn to_hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

/// Boots a not-yet-confirmed image may start before the previous image is restored.
pub const MAX_UNCONFIRMED_BOOTS: u8 = 3;

/// What to do with the image that is booting right now.
#[derive(Debug, PartialEq, Eq)]
pub enum BootDecision {
    /// Run the image normally.
    Proceed,
    /// The image never confirmed itself: restore the previous image.
    RollBack,
}

/// Software rollback for bootloaders without app rollback.
///
/// The counter is persisted by the caller: `Some(0)` is written when an update is activated
/// and the value is cleared (`None`) once the image has confirmed itself.
///
/// # Arguments
/// * `stored` - The persisted counter of unconfirmed boots, or `None` when nothing is pending.
///
/// # Returns
/// The decision for this boot and the value to persist afterwards (`None` = clear it).
pub fn on_boot(stored: Option<u8>) -> (BootDecision, Option<u8>) {
    match stored {
        None => (BootDecision::Proceed, None),
        Some(n) if n >= MAX_UNCONFIRMED_BOOTS => (BootDecision::RollBack, None),
        Some(n) => (BootDecision::Proceed, Some(n + 1)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn confirmed_images_boot_normally() {
        assert_eq!(on_boot(None), (BootDecision::Proceed, None));
    }

    #[test]
    fn an_unconfirmed_image_gets_three_boots_then_is_rolled_back() {
        let mut stored = Some(0);
        for expected in [1, 2, 3] {
            let (decision, next) = on_boot(stored);
            assert_eq!(decision, BootDecision::Proceed);
            assert_eq!(next, Some(expected));
            stored = next;
        }
        assert_eq!(on_boot(stored), (BootDecision::RollBack, None));
    }

    #[test]
    fn versions_parse() {
        assert_eq!(parse_version("0.1.0"), Some((0, 1, 0)));
        assert_eq!(parse_version("v1.2.3"), Some((1, 2, 3)));
        assert_eq!(parse_version("1.2.3-rc1+abc"), Some((1, 2, 3)));
    }

    #[test]
    fn malformed_versions_are_rejected() {
        for v in ["", "1", "1.2", "1.2.3.4", "a.b.c", "1.2.x", "-1.0.0"] {
            assert_eq!(parse_version(v), None, "{v}");
        }
    }

    #[test]
    fn versions_order_numerically() {
        assert!(parse_version("0.10.0") > parse_version("0.9.9"));
    }

    #[test]
    fn prerelease_detection() {
        assert!(is_prerelease("0.1.1-test"));
        assert!(is_prerelease("v1.0.0-fault.panic+abc"));
        assert!(!is_prerelease("0.1.1"));
        assert!(!is_prerelease("0.1.1+build-7"));
    }

    #[test]
    fn credential_compare() {
        assert!(constant_time_eq(b"Basic abc", b"Basic abc"));
        assert!(!constant_time_eq(b"Basic abc", b"Basic abd"));
        assert!(!constant_time_eq(b"Basic abc", b"Basic ab"));
    }

    #[test]
    fn hex_is_lowercase_and_padded() {
        assert_eq!(to_hex(&[0x00, 0x0f, 0xab]), "000fab");
    }
}
