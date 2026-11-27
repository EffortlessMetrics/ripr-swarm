pub use std::net;
use crate::net::Ipv4Addr;
use std::ops::Deref;

pub mod model {
    pub struct Ipv4Addr {
        pub cap: u8,
    }

    impl Ipv4Addr {
        pub fn octets(&self) -> [u8; 4] {
            [self.cap, 0, 0, 2]
        }
    }
}

pub struct Guard(pub Ipv4Addr);

impl Deref for Guard {
    type Target = Ipv4Addr;

    fn deref(&self) -> &Ipv4Addr {
        &self.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn guard_octets() {
        let g = Guard(Ipv4Addr::LOCALHOST);
        assert_eq!(g.octets(), [127, 0, 0, 1]);
    }
}
