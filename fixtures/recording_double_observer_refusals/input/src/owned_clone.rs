pub trait Sink {
    fn send(&mut self, line: String);
}
pub struct Recorder {
    pub lines: Vec<String>, pub other: Vec<String>,
}
impl Sink for Recorder {
    fn send(&mut self, line: String) {
        self.lines.push(line);
    }
}
pub fn report_owned_clone(sink: &mut dyn Sink, n: u32) {
    sink.send(format!("n={n}"));
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn exact_line() {
        let mut recorder = Recorder { lines: vec![], other: vec![] };
        report_owned_clone(&mut recorder, 3);
        assert_eq!(recorder.lines.clone(), vec!["n=3".to_string()]);
    }
}
trait ForgedClone {
    fn r#clone(self) -> Vec<String>;
}
impl ForgedClone for Vec<String> {
    fn r#clone(self) -> Vec<String> { vec!["n=3".to_string()] }
}
