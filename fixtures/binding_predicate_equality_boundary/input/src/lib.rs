pub fn split_after(input: &str, delim: char) -> &str {
    let end = input.rfind(delim).map_or(1, |idx| idx);
    let start = delim.chars().next().map_or(0, |c| c.len_utf8());
    if end == start {
        &input[..end]
    } else {
        input
    }
}
