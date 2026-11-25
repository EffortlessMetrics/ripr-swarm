pub trait Flags: Sized + Copy {
    fn bits(&self) -> u8;

    fn intersects(&self, other: Self) -> bool {
        self.bits() & other.bits() != 0
    }

    fn contains(&self, other: Self) -> bool {
        self.bits() & other.bits() != other.bits()
    }
}
