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
mod helpers;
