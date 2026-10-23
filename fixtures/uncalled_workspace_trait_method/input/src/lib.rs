pub trait Step {
    fn process(&self, x: u8) -> Result<u8, String>;
}

pub struct Inc;

impl Step for Inc {
    fn process(&self, x: u8) -> Result<u8, String> {
        match x.checked_add(1) {
            Some(v) => Ok(v),
            None => Err("ovf".to_string()),
        }
    }
}

pub fn other(x: u8) -> Result<u8, String> {
    if x > 200 {
        Err("big".to_string())
    } else {
        Ok(x)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn other_test() {
        assert_eq!(other(1), Ok(1));
        assert_eq!(other(250), Err("big".to_string()));
    }
}
