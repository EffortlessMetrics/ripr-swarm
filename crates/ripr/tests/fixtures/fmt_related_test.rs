use std::fmt;

pub struct Display { pub bytes: u64, pub unit: u64 }
impl Display {
    pub fn new(bytes: u64, unit: u64) -> Self { Self { bytes, unit } }
    pub fn configured(self) -> Self { self }
}
impl fmt::Display for Display {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let bytes = self.bytes;
        let threshold = self.unit;
        if bytes < threshold {
            write!(f, "{bytes} B")
        } else {
            write!(f, "{} kB", bytes / threshold)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fmt::Write;
    #[test]
    fn display_propagates_write_errors() {
        struct Fail;
        impl fmt::Write for Fail {
            fn write_str(&mut self, _: &str) -> fmt::Result { Err(fmt::Error) }
        }
        let mut writer = Fail;
        assert!(write!(&mut writer, "{}", Display { bytes: 1, unit: 10 }).is_err());
    }
    #[test]
    fn format_boundary() {
        assert_eq!(Display { bytes: 10, unit: 10 }.to_string(), "1 kB");
    }
    #[test]
    fn builder_boundary() {
        assert_eq!(Display::new(10, 10).configured().to_string(), "1 kB");
    }
    // SHADOW_START
    #[test]
    fn misleading_format_boundary() {
        struct Display;
        impl fmt::Display for Display {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result { write!(f, "shadow") }
        }
        assert_eq!(Display.to_string(), "shadow");
    }
    // SHADOW_END
    #[test]
    fn direct_owner_control() {
        use std::fmt::Write;
        let mut output = String::new();
        let value = super::Display { bytes: 10, unit: 10 };
        assert!(write!(&mut output, "{value}").is_ok());
        assert_eq!(output, "1 kB");
    }
}

pub fn direct_gate(x: u64) -> bool { x < 10 }
#[cfg(test)]
#[test]
fn direct_gate_test() { assert!(!direct_gate(10)); }
