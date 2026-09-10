pub(crate) const OBSERVATION_SCHEMA_GENERATION: u32 = 3;

pub fn dispatch_event(kind: &str) {
    let rendered = render_label(kind);
    log_line(&rendered);
}

pub struct CoverageRow {
    pub total: usize,
    pub skipped: usize,
}

fn render_label(kind: &str) -> String {
    format!("event:{kind}")
}

fn log_line(line: &str) {
    println!("{line}");
}
