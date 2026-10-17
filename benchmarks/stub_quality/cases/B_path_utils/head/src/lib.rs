/// Final path component.
pub fn file_name<'a>(path: &'a str) -> &'a str {
    path.rsplit('/').next().unwrap_or(path)
}

/// Extension without the dot; None for dotfiles and trailing dots.
pub fn extension<'a>(path: &'a str) -> Option<&'a str> {
    let name = file_name(path);
    let idx = name.rfind('.')?;
    if idx == 0 || idx + 1 > name.len() {
        return None;
    }
    Some(&name[idx + 1..])
}

/// Split "host:port" into parts.
pub fn split_host_port(s: &str) -> Option<(&str, u16)> {
    let (h, p) = s.rsplit_once(':')?;
    let port = p.parse::<u16>().ok()?;
    if h.is_empty() {
        return None;
    }
    Some((h, port))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn file_name_basic() {
        assert_eq!(file_name("a/b/c.txt"), "c.txt");
        assert_eq!(file_name("c.txt"), "c.txt");
    }

    #[test]
    fn extension_basic() {
        assert_eq!(extension("a/b.txt"), Some("txt"));
        assert_eq!(extension("noext"), None);
        assert_eq!(extension("dir/.bashrc"), None);
    }

    #[test]
    fn host_port() {
        assert_eq!(split_host_port("localhost:8080"), Some(("localhost", 8080)));
        assert_eq!(split_host_port("nohost"), None);
    }
}
