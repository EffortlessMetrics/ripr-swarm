//! Wall time per stage of one `ripr pilot` run (#10727).
//!
//! The table is stderr-only and verbose-only. `pilot-summary.json`, the other
//! artifacts and stdout stay free of wall-clock values, so they remain
//! byte-identical across runs (RIPR-SPEC-0185).

use std::time::Instant;

struct StageTiming {
    stage: &'static str,
    elapsed_ms: u64,
    items: Option<u64>,
}

pub(super) struct PilotTimings {
    started: Instant,
    stages: Vec<StageTiming>,
}

impl PilotTimings {
    pub(super) fn start() -> Self {
        Self {
            started: Instant::now(),
            stages: Vec::new(),
        }
    }

    /// Record `stage` as the time since `since`. `items` is the stage's own
    /// work count (files, seams) when it has one.
    pub(super) fn record(&mut self, stage: &'static str, since: Instant, items: Option<usize>) {
        self.push(stage, millis(since), items);
    }

    /// Record a stage another thread already measured.
    pub(super) fn record_ms(&mut self, stage: &'static str, elapsed_ms: u64, items: Option<usize>) {
        self.push(stage, elapsed_ms, items);
    }

    fn push(&mut self, stage: &'static str, elapsed_ms: u64, items: Option<usize>) {
        self.stages.push(StageTiming {
            stage,
            elapsed_ms,
            items: items.map(|items| u64::try_from(items).unwrap_or(u64::MAX)),
        });
    }

    /// The stage table. Stage names are static tokens and every other field
    /// is a number, so the text needs no terminal escaping.
    pub(super) fn render(&self) -> String {
        let mut out = String::from(
            "ripr pilot: stage timings (total is wall clock; rows need not sum to it)\n",
        );
        for stage in &self.stages {
            out.push_str(&format!("  {:<22} {:>9} ms", stage.stage, stage.elapsed_ms));
            if let Some(items) = stage.items {
                out.push_str(&format!("  items={items}"));
            }
            out.push('\n');
        }
        out.push_str(&format!(
            "  {:<22} {:>9} ms\n",
            "total",
            millis(self.started)
        ));
        out
    }
}

fn millis(since: Instant) -> u64 {
    u64::try_from(since.elapsed().as_millis()).unwrap_or(u64::MAX)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_table_lists_each_stage_in_order_with_its_item_count_and_a_total() {
        let mut timings = PilotTimings::start();
        timings.record_ms("load_change", 12, Some(3));
        timings.record_ms("inventory", 34_567, Some(4521));
        timings.record_ms("write_artifacts", 8, None);
        let text = timings.render();
        let lines: Vec<&str> = text.lines().collect();
        assert_eq!(lines.len(), 5, "{text}");
        assert!(lines[0].starts_with("ripr pilot: stage timings"), "{text}");
        assert_eq!(lines[1], "  load_change                   12 ms  items=3");
        assert_eq!(
            lines[2],
            "  inventory                  34567 ms  items=4521"
        );
        assert_eq!(lines[3], "  write_artifacts                8 ms");
        assert!(lines[4].starts_with("  total "), "{text}");
        assert!(lines[4].ends_with(" ms"), "{text}");
    }

    #[test]
    fn a_recorded_span_measures_the_time_since_its_start() {
        let mut timings = PilotTimings::start();
        let since = Instant::now();
        std::thread::sleep(std::time::Duration::from_millis(20));
        timings.record("inventory", since, Some(1));
        assert!(timings.stages[0].elapsed_ms >= 20, "{}", timings.render());
    }
}
