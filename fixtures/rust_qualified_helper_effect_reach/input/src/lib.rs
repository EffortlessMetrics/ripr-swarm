fn record(out: &mut Vec<u32>) {
    out.push(10);
}

pub fn wrapper(out: &mut Vec<u32>) {
    crate::record(out);
}

#[cfg(test)]
mod tests {
    use super::wrapper;

    #[test]
    fn observes_record() {
        let mut out = Vec::new();
        wrapper(&mut out);
        assert_eq!(out, vec![10]);
    }
}
