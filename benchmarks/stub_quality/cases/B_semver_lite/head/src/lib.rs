use std::fmt;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Version {
    pub major: u32,
    pub minor: u32,
    pub patch: u32,
    pub pre: Option<String>,
}

impl Version {
    pub fn new(major: u32, minor: u32, patch: u32) -> Self {
        Version { major, minor, patch, pre: None }
    }

    pub fn with_pre(mut self, pre: &str) -> Self {
        self.pre = Some(pre.to_string());
        self
    }

    pub fn bump_minor(&mut self) {
        self.minor += 1;
        self.patch = 0;
        self.pre = None;
    }
}

impl From<(u32, u32, u32)> for Version {
    fn from(t: (u32, u32, u32)) -> Self {
        Version::new(t.0, t.1, t.2)
    }
}

impl fmt::Display for Version {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match &self.pre {
            Some(pre) => write!(f, "{}.{}.{}+{}", self.major, self.minor, self.patch, pre),
            None => write!(f, "{}.{}.{}", self.major, self.minor, self.patch),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn display_plain() {
        assert_eq!(Version::from((1, 2, 3)).to_string(), "1.2.3");
    }

    #[test]
    fn display_pre_nonempty() {
        let s = Version::new(1, 2, 3).with_pre("beta").to_string();
        assert!(s.starts_with("1.2.3"));
        assert!(s.ends_with("beta"));
    }

    #[test]
    fn bump() {
        let mut v = Version::new(1, 2, 3).with_pre("rc1");
        v.bump_minor();
        assert_eq!(v, Version::new(1, 3, 0));
    }
}
