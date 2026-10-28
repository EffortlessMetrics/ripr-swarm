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
    /// Frozen by `finish`, so the table and the sidecar report one total.
    total_ms: Option<u64>,
}

impl PilotTimings {
    pub(super) fn start() -> Self {
        Self {
            started: Instant::now(),
            stages: Vec::new(),
            total_ms: None,
        }
    }

    /// Fix the wall-clock total once the last stage is recorded.
    pub(super) fn finish(&mut self) {
        self.total_ms = Some(millis(self.started));
    }

    fn total_ms(&self) -> u64 {
        self.total_ms.unwrap_or_else(|| millis(self.started))
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
        out.push_str(&format!("  {:<22} {:>9} ms\n", "total", self.total_ms()));
        out
    }
}

const TIMINGS_SCHEMA_VERSION: u32 = 1;

impl PilotTimings {
    /// The `pilot-timings.json` sidecar: the same stages, order and counts
    /// as `render`, for tools that should not parse the stderr table.
    pub(super) fn render_json(&self) -> String {
        let stages: Vec<serde_json::Value> = self
            .stages
            .iter()
            .map(|stage| {
                let mut row = serde_json::json!({
                    "stage": stage.stage,
                    "elapsed_ms": stage.elapsed_ms,
                });
                if let (Some(items), Some(object)) = (stage.items, row.as_object_mut()) {
                    object.insert("items".to_string(), serde_json::Value::from(items));
                }
                row
            })
            .collect();
        let document = serde_json::json!({
            "schema_version": TIMINGS_SCHEMA_VERSION,
            "kind": "pilot_timings",
            "stages": stages,
            "total_ms": self.total_ms(),
        });
        let mut rendered = document.to_string();
        rendered.push('\n');
        rendered
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
    fn the_json_sidecar_mirrors_the_table_stages_and_omits_absent_counts() -> Result<(), String> {
        let mut timings = PilotTimings::start();
        timings.record_ms("inventory", 34, Some(7));
        timings.record_ms("language_scan", 5, None);
        let value: serde_json::Value = serde_json::from_str(&timings.render_json())
            .map_err(|error| format!("sidecar is not JSON: {error}"))?;
        assert_eq!(value["schema_version"], 1);
        assert_eq!(value["kind"], "pilot_timings");
        assert_eq!(
            value["stages"],
            serde_json::json!([
                {"stage": "inventory", "elapsed_ms": 34, "items": 7},
                {"stage": "language_scan", "elapsed_ms": 5},
            ])
        );
        assert!(value["total_ms"].is_u64(), "{value}");
        Ok(())
    }

    #[test]
    fn a_finished_run_reports_one_total_in_the_table_and_the_sidecar() -> Result<(), String> {
        let mut timings = PilotTimings::start();
        timings.record_ms("inventory", 1, None);
        timings.finish();
        std::thread::sleep(std::time::Duration::from_millis(15));
        let value: serde_json::Value = serde_json::from_str(&timings.render_json())
            .map_err(|error| format!("sidecar is not JSON: {error}"))?;
        let table = timings.render();
        let total_row = table.lines().last().ok_or("empty table")?;
        let table_total: u64 = total_row
            .split_whitespace()
            .nth(1)
            .and_then(|ms| ms.parse().ok())
            .ok_or_else(|| format!("no total in {total_row}"))?;
        assert_eq!(value["total_ms"].as_u64(), Some(table_total), "{table}");
        assert!(
            table_total < 15,
            "total was sampled after the sleep: {table}"
        );
        Ok(())
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
