use rust_macro_table_arrow_for_pattern::*;

struct Decoy;

impl Decoy {
    #[allow(non_snake_case)]
    fn Gate(_: u32) -> bool {
        true
    }
}

macro_rules! gate_cases {
    ($($name:ident => ($input:expr, $want:expr))*) => {
        $(
            #[test]
            fn $name() {
                for gate in [Decoy::Gate] {
                    assert_eq!(gate($input), $want);
                }
            }
        )*
    };
}

// The loop binds `gate` to `Decoy::Gate`, never the changed `gate`.
gate_cases! {
    ten_is_open => (10, true)
}
