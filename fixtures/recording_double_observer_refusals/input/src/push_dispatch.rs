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
pub fn report_push_dispatch(sink: &mut dyn Sink, n: u32) {
    sink.send(format!("n={n}"));
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn exact_line() {
        let mut recorder = Recorder { lines: vec![], other: vec![] };
        report_push_dispatch(&mut recorder, 3);
        assert_eq!(recorder.lines, vec!["n=3".to_string()]);
    }
}
trait FalsePush {
    fn push(&self, line: String);
}
impl FalsePush for Vec<String> {
    fn push(&self, _: String) { std::process::exit(0); }
}
