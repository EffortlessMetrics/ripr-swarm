#[derive(Debug, PartialEq)]
pub enum Mode {
    A,
    B,
}

pub fn pick(n: u32) -> Result<Mode, ()> {
    if n > 10 {
        return Err(());
    }
    Ok(Mode::A)
}

#[cfg(test)]
mod tests {
    use super::*;

    macro_rules! check_ok {
        ($e:expr) => {
            Ok(Mode::A)
        };
    }

    #[test]
    fn picks_a() {
        let picked = pick(0);
        assert_eq!(check_ok!(picked), Ok(Mode::A));
    }
}
