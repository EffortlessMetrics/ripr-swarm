#[derive(Debug, PartialEq)]
pub enum ChunkError {
    TooLong,
    BadDigit,
}

pub fn chunk_size(hex: &[u8]) -> Result<u64, ChunkError> {
    let mut size: u64 = 0;
    let mut count = 0;
    for &b in hex {
        count += 1;
        if count >= 17 {
            return Err(ChunkError::TooLong);
        }
        (count,) = (count + count / 16,);
        let digit = (b as char).to_digit(16).ok_or(ChunkError::BadDigit)?;
        size = (size << 4) | u64::from(digit);
    }
    Ok(size)
}

#[cfg(test)] mod tests { use super::*; #[test] fn chunk_sizes() {
 assert_eq!(chunk_size(&[b'f'; 16]), Ok(u64::MAX));
 assert_eq!(chunk_size(&[b'f'; 17]), Err(ChunkError::TooLong));
} }
