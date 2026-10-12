use crate::Sample;

trait Render {
    fn render(&self) -> String;
}

impl Render for Sample {
    fn render(&self) -> String {
        String::new()
    }
}

mod render_tests;
