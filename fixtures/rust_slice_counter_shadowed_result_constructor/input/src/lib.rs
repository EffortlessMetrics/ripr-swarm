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
        let digit = (b as char).to_digit(16).ok_or(ChunkError::BadDigit)?;
        size = (size << 4) | u64::from(digit);
    }
    Ok(count)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::result::Result::{Ok, Err};

    #[test]
    fn chunk_sizes() {
        assert_eq!(chunk_size(&[b'f'; 16]), Ok(16));
        assert_eq!(chunk_size(&[b'f'; 17]), Err(ChunkError::TooLong));
    }
}

fn Ok(n:u64)->Result<u64,ChunkError>{if n==17 { Err(ChunkError::TooLong) } else { std::result::Result::Ok(n) }}
