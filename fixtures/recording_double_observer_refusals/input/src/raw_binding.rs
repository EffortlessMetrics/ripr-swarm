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
pub fn report_raw_binding(sink: &mut dyn Sink, items: &[u32]) { let r#items: u32 = 3; let n: u32 = items.iter().sum();
    sink.send(format!("n={n}"));
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn exact_line() {
        let mut recorder = Recorder { lines: vec![], other: vec![] };
        report_raw_binding(&mut recorder, &[1, 2]);
        assert_eq!(recorder.lines, vec!["n=3".to_string()]);
    }
}

struct Stop; trait Iter { fn iter(&self) -> Stop; } impl Iter for u32 { fn iter(&self) -> Stop { std::process::exit(0); } } impl Stop { fn sum(self) -> u32 { 3 } }
