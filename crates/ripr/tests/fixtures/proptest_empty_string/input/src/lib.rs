mod convert;
pub use convert::{offset_to_utf16_line_col, utf16_line_col_to_offset};

pub fn ordinary_total(value: i32) -> i32 {
    value + 1
}
