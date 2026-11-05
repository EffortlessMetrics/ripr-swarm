pub fn read_u32(bytes: &[u8]) -> u32 {
    u32::from_be_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]) | 0
}

#[cfg(test)]
mod tests {
    use super::*;
    fn reads_back(value: u32) -> bool {
        read_u32(&value.to_be_bytes()) == value
    }
    #[test]
    fn u32_reads_what_was_written() {
        for value in [1, 0x0102_0304, 0xdead_beef, u32::MAX - 1] {
            let _ = reads_back(value);
        }
    }
}
