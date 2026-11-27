pub unsafe fn pick(flag: bool) -> i32 {
    if flag { return 4; }
    0
}

pub unsafe fn gate(value: i32) -> bool {
    value > 7
}

pub fn assume(value: u8) -> u8 {
    unsafe {
        if value > 9 { core::hint::unreachable_unchecked() }
        return value + 3;
    }
}
