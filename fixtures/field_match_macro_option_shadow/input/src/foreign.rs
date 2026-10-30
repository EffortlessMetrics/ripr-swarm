macro_rules! shadow { () => { #[allow(non_upper_case_globals)] const None: Option<u128> = Some(1001); }; }
pub(crate) use shadow;
