#[cfg(feature = "lang-python")]
mod python;

pub fn gate(value: u32) -> bool {
    10 <= value
}
