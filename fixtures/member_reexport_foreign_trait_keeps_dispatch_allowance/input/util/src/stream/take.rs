use rmid::{Display, FmtResult, Formatter};

pub struct Take {
    remaining: usize,
}

impl Display for Take {
    fn fmt(&self, f: &mut Formatter<'_>) -> FmtResult {
        if self.remaining <= 4 {
            write!(f, "few")
        } else {
            write!(f, "many")
        }
    }
}
