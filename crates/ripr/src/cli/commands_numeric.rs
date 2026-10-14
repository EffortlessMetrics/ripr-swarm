//! Shared numeric flag parsing.
//!
//! Errors follow the `--git-timeout` house shape (`check.rs`): the flag, the
//! expected shape, and the typed value verbatim. Leaking std parse text
//! (`invalid --x: invalid digit found in string`) or dropping the value
//! (`invalid --x: expected a positive integer`) leaves the user guessing
//! which of their arguments was wrong (#4318).

fn parse_positive_integer<T>(value: &str, flag: &str) -> Result<T, String>
where
    T: std::str::FromStr + Eq + From<u8>,
{
    let parsed = value
        .parse::<T>()
        .map_err(|_parse_err| format!("{flag} requires a positive integer; got {value:?}"))?;
    if parsed == T::from(0) {
        return Err(format!("{flag} requires a positive integer; got {value:?}"));
    }
    Ok(parsed)
}

pub(super) fn parse_positive_usize(value: &str, flag: &str) -> Result<usize, String> {
    parse_positive_integer::<usize>(value, flag)
}

/// Caps like `--max-related-tests` are render knobs where zero is a valid
/// explicit per-invocation input: the context packet then renders zero
/// related tests (#4318 review). The `reports.max_related_tests` config key
/// is different — a zero there used to be accepted silently and emptied
/// every packet's evidence — and now fails closed at config parse (#7353);
/// the explicit flag keeps its deliberate per-invocation contract. Only a
/// malformed or overflowing value is an error here.
pub(super) fn parse_non_negative_usize(value: &str, flag: &str) -> Result<usize, String> {
    value
        .parse::<usize>()
        .map_err(|_parse_err| format!("{flag} requires a non-negative integer; got {value:?}"))
}

pub(super) fn parse_positive_u64(value: &str, flag: &str) -> Result<u64, String> {
    parse_positive_integer::<u64>(value, flag)
}
