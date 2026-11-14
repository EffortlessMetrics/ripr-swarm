trait Render {
    fn render(&self) -> String;
}

impl Render for f64 {
    fn render(&self) -> String {
        String::new()
    }
}

mod render_tests;
