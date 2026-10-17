#[cfg_attr(all(), doc = r#"Example: ", no_mangle, " tail"#)]
pub fn ordinary(amount: u64) -> u64 {
    if amount >= 100 { 5 } else { 0 }
}

#[cfg_attr(all(), doc = r#"An unmatched quote: " ("#, unsafe(no_mangle))]
pub fn exported(amount: u64) -> u64 {
    if amount >= 100 { 5 } else { 0 }
}

#[cfg_attr(all(), doc = "plain", /* , no_mangle, */ inline)]
pub fn commented(amount: u64) -> u64 {
    if amount >= 100 { 5 } else { 0 }
}
