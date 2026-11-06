pub trait Render {
    fn render(&self) -> String;
}

impl Render for f64 {
    fn render(&self) -> String {
        String::from("")
    }
}

#[cfg(test)]
mod helpers;
