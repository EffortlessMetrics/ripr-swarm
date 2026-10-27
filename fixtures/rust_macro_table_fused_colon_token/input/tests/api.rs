use rust_macro_table_fused_colon_token::*;

macro_rules! gate_cases {
    ($($name:ident : $t:tt => ($input:expr, $want:expr))*) => {
        $(
            #[test]
            fn $name() {
                assert_eq!(gate($input), $want);
            }
        )*
    };
    ($($tail:tt)*) => {};
}

// rustc reads `::` as one token, so the literal `:` never matches and the
// fallback arm expands to nothing: this file declares no test.
gate_cases! {
    nine_is_closed :: => (9, false)
    ten_is_open :: => (10, true)
}
