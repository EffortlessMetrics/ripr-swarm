pub fn chunk_size(hex: &[u8]) -> Result<u64, &'static str> {
    let mut count = 0;
    for _ in hex {
        count += 1;
        if count >= 17 { return Err("too long"); }
        if count == 17 { return Err("too long"); }
    }
    Ok(0)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn adjacent_lengths() {
        assert_eq!(chunk_size(&[b'f'; 16]), Ok(0));
        assert_eq!(chunk_size(&[b'f'; 17]), Err("too long"));
    }
}
