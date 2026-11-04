pub trait Render {
    fn render(&self) -> String;
}

impl Render for f64 {
    fn render(&self) -> String {
        String::from("")
    }
}

#[cfg(test)]
mod tests {
    use super::Render;

    #[test]
    fn render_is_empty() {
        assert_eq!((-0.0f64).render(), String::from(""));
    }
}
