//! A literal's source length does not prove a `.to_string()` result's length.
#![no_implicit_prelude]

extern crate std;
use std::string::String;

trait ToString {
    fn to_string(&self) -> String;
}

impl ToString for str {
    fn to_string(&self) -> String {
        String::new()
    }
}

fn before() -> String {
    "checks catalog consistency".to_string()
}

fn after() -> String {
    "verifies catalog consistency".to_string()
}

#[test]
fn custom_conversion_erases_the_changed_literal_length() {
    // Both versions execute the exact same `.len() == 0` assertion. A rule
    // that projects the source literal's length through `.to_string()` would
    // report a difference that the returned value does not carry.
    std::assert_eq!(before().len(), 0);
    std::assert_eq!(after().len(), 0);
    // The standard conversion is a distinct, explicitly selected control.
    std::assert_eq!(
        std::string::ToString::to_string("checks catalog consistency").len(),
        26
    );
    std::assert_eq!(
        std::string::ToString::to_string("verifies catalog consistency").len(),
        28
    );
}
