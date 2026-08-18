pub fn body_of(label: &str) -> &str {
    let body = label.strip_prefix("pre-").map_or("none", |s| s);
    if body == "fix" {
        "matched"
    } else {
        "other"
    }
}

pub fn gate(label: &str, want: bool) -> bool {
    let open = label.starts_with("pre-");
    open == want
}

pub fn within(input: &str, other: &str) -> bool {
    let size = other.len();
    size == 3
}

pub fn bump(base: usize) -> bool {
    let next = base.checked_add(1).map_or(0, |v| v);
    next == 4
}
