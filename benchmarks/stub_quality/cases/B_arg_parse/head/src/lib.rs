use std::fmt;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Opts {
    pub name: String,
    pub jobs: usize,
    pub verbose: bool,
}

impl Opts {
    pub fn new(name: impl Into<String>) -> Self {
        Opts {
            name: name.into(),
            jobs: 2,
            verbose: false,
        }
    }

    pub fn set_jobs(&mut self, n: usize) {
        self.jobs = n.max(1);
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ArgError {
    MissingValue(String),
    UnknownFlag(String),
    BadNumber(String),
}

impl fmt::Display for ArgError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ArgError::MissingValue(a) => write!(f, "missing value for {a}"),
            ArgError::UnknownFlag(a) => write!(f, "unknown flag {a}"),
            ArgError::BadNumber(a) => write!(f, "bad number {a}"),
        }
    }
}

impl std::error::Error for ArgError {}

pub fn parse_args(args: &[String]) -> Result<Opts, ArgError> {
    let mut opts = Opts::new("app");
    let mut it = args.iter();
    while let Some(a) = it.next() {
        match a.as_str() {
            "-v" => opts.verbose = true,
            "-j" => {
                let v = it.next().ok_or_else(|| ArgError::MissingValue(a.clone()))?;
                let n = v.parse::<usize>().map_err(|_| ArgError::BadNumber(v.clone()))?;
                opts.set_jobs(n);
            }
            s if s.starts_with('-') => return Err(ArgError::UnknownFlag(s.to_string())),
            s => opts.name = s.to_string(),
        }
    }
    Ok(opts)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn v(a: &[&str]) -> Vec<String> {
        a.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn new_takes_str_and_string() {
        assert_eq!(Opts::new("x").name, "x");
        assert_eq!(Opts::new(String::from("y")).name, "y");
    }

    #[test]
    fn parses_flags() {
        let o = parse_args(&v(&["prog", "-v", "-j", "4"])).unwrap();
        assert_eq!(o.name, "prog");
        assert!(o.verbose);
        assert_eq!(o.jobs, 4);
    }

    #[test]
    fn errors() {
        assert!(parse_args(&v(&["-j"])).is_err());
        assert!(parse_args(&v(&["--nope"])).is_err());
        assert!(parse_args(&v(&["-j", "x"])).is_err());
    }
}
