#[derive(Debug)]
pub struct E;

fn digit(c: char) -> Result<u32, E> {
    c.to_digit(10).ok_or(E)
}

pub fn total(s: &str) -> Result<u32, E> {
    let mut sum = 0;
    for c in s.chars() {
        let d = digit(c)?;
        sum += d;
    }
    Ok(sum)
}
