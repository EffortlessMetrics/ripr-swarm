//! Canonical Cargo-selected compiler identity shared with the scoreboard.
//! This is build evidence, not a declaration of the package's source MSRV.

pub(crate) const ENV: &str = "RIPR_BUILD_RUSTC";

/// A complete record binds the release, full source commit, compiler host and
/// LLVM version. Unknown fields never establish an identity, even when two
/// reports carry the same unknown spelling.
pub(crate) fn parse(record: &str) -> Option<[&str; 4]> {
    if record.len() > 512 {
        return None;
    }
    let fields: Vec<_> = record.split('|').collect();
    let ["rustc-v1", release, commit, host, llvm] = fields.as_slice() else {
        return None;
    };
    let version = |text: &str| {
        let numbers = text.split('-').next().unwrap_or("");
        numbers.split('.').count() == 3
            && numbers
                .split('.')
                .all(|part| !part.is_empty() && part.bytes().all(|byte| byte.is_ascii_digit()))
            && text.len() <= 80
            && text
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || b".-+".contains(&byte))
    };
    if !version(release)
        || !version(llvm)
        || commit.len() != 40
        || !commit.bytes().all(|byte| byte.is_ascii_hexdigit())
        || host.len() > 120
        || !host.contains('-')
        || host.split('-').any(str::is_empty)
        || !host
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || b"_-".contains(&byte))
    {
        return None;
    }
    Some([release, commit, host, llvm])
}
