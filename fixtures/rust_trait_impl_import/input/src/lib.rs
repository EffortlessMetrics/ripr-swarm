pub trait Render {
    fn render(&self) -> String;
}

impl Render for Sample {
    fn render(&self) -> String {
        String::from("")
    }
}

pub struct Sample;

#[cfg(test)]
mod tests {
    use super::{Render, Sample};

    #[test]
    fn render_is_empty() {
        assert_eq!(Sample.render(), String::from(""));
    }
}
