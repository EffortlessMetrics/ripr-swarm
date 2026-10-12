pub struct Packet {
    pub Upper: u8,
}

pub fn packet<T>(value: T) -> Packet
where
    T: Into<u8> + Copy,
{
    Packet {
        Upper: value.into(),
    }
}

#[cfg(test)]
mod tests {
    use super::packet;

    #[test]
    fn observes_packet() {
        assert_eq!(packet(7_u8).Upper, 7);
    }
}
