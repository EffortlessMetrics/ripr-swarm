/// Lowercase ASCII slug: runs of non-alphanumerics become one `-`, no leading
/// or trailing `-`, at most `MAX_LEN` characters (never ending in `-`).
pub const MAX_LEN: usize = 20;

pub fn slugify(input: &str) -> String {
    let mut out = String::new();
    let mut pending_dash = false;
    for c in input.chars() {
        if c.is_ascii_alphanumeric() {
            if pending_dash && !out.is_empty() {
                out.push('-');
            }
            pending_dash = false;
            out.push(c.to_ascii_lowercase());
        } else {
            pending_dash = true;
        }
    }
    if out.len() > MAX_LEN {
        out.truncate(MAX_LEN);
        while out.ends_with('-') {
            out.pop();
        }
    }
    out
}
