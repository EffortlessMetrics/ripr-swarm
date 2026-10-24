use rmid::Display;

pub struct Take {
    remaining: usize,
}

impl Display for Take {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if self.remaining <= 4 {
            write!(f, "few")
        } else {
            write!(f, "many")
        }
    }
}
