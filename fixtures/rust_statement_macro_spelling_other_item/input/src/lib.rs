pub fn suit_color(suit: char) -> &'static str {
    match suit {
        'H' | 'D' => "red",
        'S' | 'C' => "black",
        _ => "unknown",
    }
}

#[macro_export]
macro_rules! emit {
    ($($item:tt)*) => {
        $($item)*
    };
}
