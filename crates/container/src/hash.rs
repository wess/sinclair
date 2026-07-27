//! A tiny non-cryptographic hash, used to derive stable names from paths and
//! image recipes. FNV-1a: it only has to separate two inputs, not resist
//! anyone, and it keeps this crate dependency-free.

/// Eight hex characters of FNV-1a over `s`.
pub(crate) fn short(s: &str) -> String {
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    for b in s.as_bytes() {
        hash ^= *b as u64;
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    format!("{hash:016x}")[..8].to_string()
}
