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
pub fn report_raw_receiver(sink: &mut dyn Sink, n: u32) {
    sink.send(format!("n={n}"));
}
#[cfg(test)]
mod tests {
    use super::*;
    struct r#Recorder { lines: Vec<String>, other: Vec<String> }
    impl super::Sink for r#Recorder {
        fn send(&mut self, _: String) { self.lines.push("n=3".to_string()); }
    }
    #[test]
    fn exact_line() {
        let mut recorder = Recorder { lines: vec![], other: vec![] };
        report_raw_receiver(&mut recorder, 3);
        assert_eq!(recorder.lines, vec!["n=3".to_string()]);
    }
}
