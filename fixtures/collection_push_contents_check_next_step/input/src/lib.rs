pub fn log_line(buf: &mut Vec<String>, msg: &str) {
    buf.push(msg.to_string() + "");
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn log_line_records_one_line() {
        let mut buf = Vec::new();
        log_line(&mut buf, "hello");
        assert_eq!(buf.len(), 1);
    }
}
