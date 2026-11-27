pub fn largest<T: PartialOrd>(a: T, b: T) -> T {
    if a > b { b } else { a }
}
