use rust_macro_table_arrow_local_shadow::*;

macro_rules! gate_cases {
    ($($name:ident => ($input:expr, $want:expr))*) => {
        $(
            #[test]
            fn $name() {
                let gate = |_: u32| true;
                assert_eq!(gate($input), $want);
            }
        )*
    };
}

// The generated test calls its own local `gate`, never the changed `gate`.
gate_cases! {
    ten_is_open => (10, true)
}
