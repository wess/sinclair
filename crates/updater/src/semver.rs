//! Version parsing and comparison for release tags.

/// Parse `major.minor.patch` (tolerating a leading `v` and extra fields).
fn parse(v: &str) -> Option<(u64, u64, u64)> {
    let v = v.trim().trim_start_matches('v');
    let mut it = v.split(['.', '-', '+']).map(|p| p.parse::<u64>().ok());
    Some((it.next()??, it.next()??, it.next()??))
}

/// Whether `latest` is a strictly newer version than `current`. Anything that
/// doesn't parse as a version is never newer.
pub fn is_newer(latest: &str, current: &str) -> bool {
    match (parse(latest), parse(current)) {
        (Some(a), Some(b)) => a > b,
        _ => false,
    }
}

#[cfg(test)]
#[path = "../tests/semver.rs"]
mod tests;
