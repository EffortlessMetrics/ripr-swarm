macro_rules! threshold_fn {
    ($name:ident, $limit:expr) => {
        pub fn $name(temp_c: i32) -> bool {
            temp_c >= $limit
        }
    };
}

threshold_fn!(is_hot, 31);
threshold_fn!(is_freezing, 0);

pub(crate) fn internal_label(t: i32) -> &'static str {
    if is_hot(t) { "hot" } else { "ok" }
}

#[cfg(feature = "extra")]
pub fn extra_label(t: i32) -> String {
    format!("{}C:{}", t, internal_label(t))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn hot() {
        assert!(is_hot(45));
        assert!(!is_hot(10));
        assert!(is_freezing(0));
    }
    #[test]
    fn label() {
        assert_eq!(internal_label(10), "ok");
    }
}
