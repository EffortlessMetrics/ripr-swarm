#[derive(Debug, PartialEq)]
pub enum Kind {
    Alpha,
    Beta,
}

pub fn flip(k: Kind) -> Kind {
    match k {
        Kind::Alpha => Kind::Beta,
        Kind::Beta => Kind::Alpha,
    }
}
