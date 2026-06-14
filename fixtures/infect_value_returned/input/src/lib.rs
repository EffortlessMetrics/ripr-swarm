pub fn score(amount: i32) -> i32 {
    let result = helper(amount * 9);
    result + 1
}

pub fn helper(x: i32) -> i32 {
    x / 10
}
