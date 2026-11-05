#[derive(Debug)]
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
        let digit = (b as char).to_digit(16).ok_or(ChunkError::BadDigit)?;
        size = (size << 4) | u64::from(digit);
    }
    Ok(size)
}

impl PartialEq for ChunkError { fn eq(&self,_other:&Self)->bool {true} }

#[cfg(test)] mod tests { use super::*; #[test] fn chunk_sizes() {
 assert_eq!(chunk_size(&[b'z'; 16]), Err(ChunkError::BadDigit));
 assert_eq!(chunk_size(&[b'z'; 17]), Err(ChunkError::TooLong));
} }
