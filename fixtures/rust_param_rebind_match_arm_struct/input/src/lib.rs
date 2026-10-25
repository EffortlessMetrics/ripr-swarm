pub struct Foo {
    pub n: usize,
}

pub fn score(n: usize, k: Option<Foo>) -> u32 {
    match k {
        Some(Foo { n, .. }) => drop(n),
        None => {}
    }
    let m = n * 2;
    if m == 40 {
        return 0;
    }
    1
}
