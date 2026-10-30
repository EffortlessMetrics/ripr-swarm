pub(crate) use std::collections::BTreeSet;

pub union ChangedItem {
    pub changed_slot: u64,
}

macro_rules! changed_item {
    () => {
        3
    };
}

pub const SEED_BASE: u32 = 1;

pub const PICKED: u32 = SEED_BASE;

pub fn unrelated() -> u32 {
    7
}
