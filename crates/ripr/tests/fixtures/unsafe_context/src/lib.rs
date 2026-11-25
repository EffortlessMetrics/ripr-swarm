pub fn accepts(value: i32) -> bool {
    unsafe {
        if value >= 3 {
            return true;
        }
    }
    false
}

pub unsafe fn score(value: i32) -> i32 {
    if value < 0 {
        return value + 2;
    }
    let bias: i32 = 3;
    value + bias
}

pub fn inline(value: i32) -> i32 {
    unsafe { value + 2 }
}

pub fn choose(enabled: bool) -> i32 {
    let mut value = 0;
    unsafe {
        if enabled { value = 2; }
    };
    value
}

pub fn span(value: i32) -> i32 {
    unsafe {
        let low: i32 = 2;
        let high: i32 = 6;
        low + high + value
    }
}

pub fn exact(value: i32) -> i32 {
    unsafe { value + 16 }
}
