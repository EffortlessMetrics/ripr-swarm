pub fn scale(value: f64, factor: f64) -> f64 {
    if value > factor {
        value * factor
    } else {
        factor
    }
}
