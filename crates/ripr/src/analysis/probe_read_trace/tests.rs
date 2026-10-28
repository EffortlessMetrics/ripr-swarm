use super::*;
use crate::domain::{DeltaKind, ProbeFamily, ProbeId, SourceLocation};

fn capture<T>(enabled: bool, work: impl FnOnce() -> T) -> Result<(T, Vec<Observation>), String> {
    struct Restore(Option<TestCapture>);
    impl Drop for Restore {
        fn drop(&mut self) {
            CAPTURE.with(|slot| {
                if let Ok(mut capture) = slot.try_borrow_mut() {
                    *capture = self.0.take();
                }
            });
        }
    }
    let restore = Restore(CAPTURE.with(|slot| {
        slot.try_borrow_mut()
            .map(|mut current| {
                current.replace(TestCapture {
                    enabled,
                    rows: Vec::new(),
                })
            })
            .map_err(|error| error.to_string())
    })?);
    let result = work();
    let rows = CAPTURE.with(|slot| {
        let mut current = slot.try_borrow_mut().map_err(|error| error.to_string())?;
        let capture = current.as_mut().ok_or("capture disappeared")?;
        Ok::<_, String>(std::mem::take(&mut capture.rows))
    })?;
    drop(restore);
    Ok((result, rows))
}

fn probe(id: &str) -> Probe {
    Probe {
        id: ProbeId(id.to_string()),
        location: SourceLocation::new("src/lib.rs", 1, 1),
        owner: None,
        family: ProbeFamily::Predicate,
        delta: DeltaKind::Value,
        before: None,
        after: None,
        expression: "total >= 10".to_string(),
        expected_sinks: Vec::new(),
        required_oracles: Vec::new(),
    }
}

#[test]
fn scope_is_per_invocation_and_disabled_or_outside_reads_emit_nothing() -> Result<(), String> {
    let (_, disabled) = capture(false, || {
        let scope = Scope::begin(&probe("disabled"));
        universe(Universe::Files);
        scope.finish();
    })?;
    assert!(disabled.is_empty());
    let (_, rows) = capture(true, || {
        file(Path::new("outside.rs"), true);
        let first = Scope::begin(&probe("first"));
        for _ in 0..2 {
            file(Path::new("first.rs"), true);
        }
        first.finish();
        let second = Scope::begin(&probe("second"));
        file(Path::new("missing.rs"), false);
        second.finish();
    })?;
    assert_eq!(rows.len(), 2);
    assert_eq!(rows[0].probe_id.as_deref(), Some("first"));
    assert_eq!(rows[1].probe_id.as_deref(), Some("second"));
    assert_eq!(rows[0].reads.len(), 1);
    assert_eq!(rows[0].reads[0].read.kind, Kind::File);
    assert_eq!(rows[0].reads[0].read.subject, "first.rs");
    assert_eq!(rows[0].reads[0].count, 2);
    assert_eq!(rows[0].observed_read_events, 2);
    assert_eq!(rows[1].reads[0].read.kind, Kind::Absent);
    assert_eq!(rows[1].reads[0].count, 1);
    assert_eq!(rows[1].observed_read_events, 1);
    assert!(!rows.iter().any(|row| {
        row.reads
            .iter()
            .any(|read| read.read.subject == "outside.rs")
    }));
    assert!(
        rows.iter()
            .all(|row| row.classification_returned && row.coverage == "unknown" && !row.reusable)
    );
    Ok(())
}

#[test]
fn nested_scope_restores_parent_and_interruption_never_becomes_completion() -> Result<(), String> {
    let token = crate::analysis::cancellation::AnalysisCancellationToken::new();
    let (_, rows) = capture(true, || {
        crate::analysis::cancellation::with_optional_token(Some(&token), || {
            let parent = Scope::begin(&probe("parent"));
            file(Path::new("before.rs"), true);
            let child = Scope::begin(&probe("child"));
            file(Path::new("child.rs"), true);
            child.finish();
            file(Path::new("after.rs"), true);
            token.cancel(crate::analysis::cancellation::AnalysisAbortKind::Cancelled);
            parent.finish();
        });
    })?;
    assert_eq!(rows.len(), 2);
    assert_eq!(rows[0].probe_id.as_deref(), Some("child"));
    assert_eq!(rows[1].probe_id.as_deref(), Some("parent"));
    assert_eq!(rows[0].reads.len(), 1);
    assert_eq!(rows[1].reads.len(), 2);
    assert!(rows[1].interrupted);
    assert_eq!(rows[1].analysis_completion, "not_established");
    assert!(!rows[1].reusable);
    Ok(())
}

#[test]
fn scopes_do_not_cross_worker_threads() -> Result<(), String> {
    let handles = ["worker-a", "worker-b"].map(|id| {
        std::thread::spawn(move || {
            capture(true, || {
                let scope = Scope::begin(&probe(id));
                file(Path::new(id), true);
                scope.finish();
            })
        })
    });
    for (handle, expected) in handles.into_iter().zip(["worker-a", "worker-b"]) {
        let (_, rows) = handle
            .join()
            .map_err(|panic| format!("recording worker failed: {panic:?}"))??;
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].probe_id.as_deref(), Some(expected));
        assert_eq!(rows[0].reads[0].read.subject, expected);
    }
    Ok(())
}

#[test]
fn abandonment_and_saturation_never_hide_whole_set_reads_or_claim_closure() -> Result<(), String> {
    let (_, rows) = capture(true, || {
        let _abandoned = Scope::begin(&probe("bounded"));
        for index in 0..MAX_READS + 10 {
            file(Path::new(&format!("source-{index}.rs")), true);
        }
        universe(Universe::Tests);
    })?;
    assert_eq!(rows.len(), 1);
    let row = &rows[0];
    assert!(!row.classification_returned);
    assert!(row.universe_observed);
    assert_eq!(row.reads.len(), MAX_READS);
    assert_eq!(row.dropped_read_events, 11);
    assert_eq!(row.analysis_completion, "not_established");
    assert_eq!(row.coverage, "unknown");
    assert!(!row.reusable);
    assert_eq!(row.unknown_inputs, UNKNOWN_INPUTS);
    Ok(())
}

#[test]
fn site_tallies_keep_distinct_reads_visible_past_the_detail_cap() -> Result<(), String> {
    let (_, rows) = capture(true, || {
        let scope = Scope::begin(&probe("many"));
        for index in 0..MAX_READS + 36 {
            // Same subject twice: events double, distinct subjects do not.
            for _ in 0..2 {
                file(Path::new(&format!("distinct-{index}.rs")), true);
            }
        }
        scope.finish();
    })?;
    let row = rows.first().ok_or("missing row")?;
    assert_eq!(row.reads.len(), MAX_READS);
    assert_eq!(row.sites.len(), 1);
    let site = &row.sites[0];
    assert_eq!(site.kind, Kind::File);
    assert_eq!(site.events, 2 * (MAX_READS as u64 + 36));
    assert_eq!(site.distinct_subjects, MAX_READS as u64 + 36);
    assert!(!site.distinct_is_lower_bound);
    assert_eq!(row.dropped_site_events, 0);
    Ok(())
}

#[test]
fn distinct_count_is_a_lower_bound_once_its_memory_bound_is_reached() -> Result<(), String> {
    let (_, rows) = capture(true, || {
        let scope = Scope::begin(&probe("saturated"));
        for index in 0..MAX_DISTINCT + 5 {
            file(Path::new(&format!("s-{index}.rs")), true);
        }
        scope.finish();
    })?;
    let site = rows
        .first()
        .and_then(|row| row.sites.first())
        .ok_or("missing site")?;
    assert_eq!(site.events, MAX_DISTINCT as u64 + 5);
    assert_eq!(site.distinct_subjects, MAX_DISTINCT as u64);
    assert!(site.distinct_is_lower_bound);
    Ok(())
}

#[test]
fn wire_bound_omits_whole_details_instead_of_truncating_and_writer_failure_is_fallible()
-> Result<(), String> {
    let (_, rows) = capture(true, || {
        let scope = Scope::begin(&probe("escaped"));
        for index in 0..MAX_READS {
            file(Path::new(&format!("{index}{}", "\"\\".repeat(240))), true);
        }
        scope.finish();
    })?;
    let row = rows.into_iter().next().ok_or("missing escaped row")?;
    let mut wire = Vec::new();
    write_observation(&mut wire, row.clone()).map_err(|error| error.to_string())?;
    assert!(wire.len() <= MAX_ROW_BYTES + PREFIX.len() + 1);
    let value: serde_json::Value =
        serde_json::from_slice(&wire[PREFIX.len()..]).map_err(|error| error.to_string())?;
    assert_eq!(value["read_detail_omitted"], true);
    assert_eq!(value["reads"], serde_json::json!([]));
    // Site totals are not read detail and survive the omission.
    assert_eq!(value["sites"][0]["distinct_subjects"], MAX_READS);
    assert_eq!(value["coverage"], "unknown");
    struct Closed;
    impl Write for Closed {
        fn write(&mut self, _bytes: &[u8]) -> io::Result<usize> {
            Err(io::ErrorKind::BrokenPipe.into())
        }
        fn flush(&mut self) -> io::Result<()> {
            Ok(())
        }
    }
    assert_eq!(
        write_observation(&mut Closed, row.clone())
            .err()
            .map(|error| error.kind()),
        Some(io::ErrorKind::BrokenPipe)
    );
    struct Partial(Vec<u8>);
    impl Write for Partial {
        fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
            if self.0.len() >= PREFIX.len() + 3 {
                return Err(io::ErrorKind::BrokenPipe.into());
            }
            let count = bytes.len().min(PREFIX.len() + 3 - self.0.len());
            self.0.extend_from_slice(&bytes[..count]);
            Ok(count)
        }
        fn flush(&mut self) -> io::Result<()> {
            Ok(())
        }
    }
    let mut partial = Partial(Vec::new());
    assert_eq!(
        write_observation(&mut partial, row)
            .err()
            .map(|error| error.kind()),
        Some(io::ErrorKind::BrokenPipe)
    );
    if let Ok(value) = serde_json::from_slice::<serde_json::Value>(&partial.0[PREFIX.len()..]) {
        return Err(format!(
            "partial diagnostic unexpectedly parsed as complete JSON: {value}"
        ));
    }
    Ok(())
}

#[cfg(feature = "lang-rust")]
mod producer;
