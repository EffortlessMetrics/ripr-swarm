use std::fmt;

pub struct Temp(pub i32);

impl fmt::Display for Temp {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, r#"quoted"marker"{:?}°C"#, self.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn exact_display_pin() {
        assert_eq!(Temp(21).to_string(), "quoted\"marker\"21°C");
    }
}
