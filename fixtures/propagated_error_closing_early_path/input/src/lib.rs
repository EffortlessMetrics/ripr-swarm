#[derive(Debug, PartialEq)]
pub enum Failure { Missing }

pub fn check(early: bool, value: Option<()>) -> Result<(), Failure> {
    if early { return Err(Failure::Missing); }
    value.ok_or(
        Failure::Missing
    )?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn observes_only_the_early_error() {
        assert_eq!(check(true, None), Err(Failure::Missing));
    }
}
