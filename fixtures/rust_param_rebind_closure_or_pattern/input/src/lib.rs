pub enum Either {
    Left(usize),
    Right(usize),
}

pub fn score(n: usize) -> u32 {
    let f = |(Either::Left(n) | Either::Right(n)): Either| {
        let m = n * 2;
        if m == 40 {
            return 0;
        }
        1
    };
    let _ = n;
    f(Either::Left(7))
}
