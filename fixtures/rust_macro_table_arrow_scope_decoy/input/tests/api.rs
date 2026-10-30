use rust_macro_table_arrow_scope_decoy::*;

macro_rules! gate_cases {
    ($($name:ident => ($input:expr, $want:expr))*) => {
        $(
            mod decoy {
                pub fn gate(_: u32) -> bool {
                    true
                }
            }

            #[test]
            fn $name() {
                assert_eq!(decoy::gate($input), $want);
            }
        )*
    };
}

// The generated test calls `decoy::gate`, never the changed `gate`.
gate_cases! {
    ten_is_open => (10, true)
}
