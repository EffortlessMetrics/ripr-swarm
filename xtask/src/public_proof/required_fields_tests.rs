use super::*;

fn workspace_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .map_or_else(|| PathBuf::from("."), Path::to_path_buf)
}

/// Exercise the same disk receipt loader and renderer as `public-proof`, while
/// leaving committed receipts and the generated page untouched.
fn with_receipt_copy(test: impl FnOnce(&Path) -> Result<(), String>) -> Result<(), String> {
    static NEXT: AtomicUsize = AtomicUsize::new(0);
    let scratch = std::env::temp_dir().join(format!(
        "ripr-public-proof-required-fields-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ));
    let result = (|| {
        let real = workspace_root();
        let receipts = scratch.join(RECEIPTS);
        fs::create_dir_all(&receipts).map_err(|e| e.to_string())?;
        for entry in fs::read_dir(real.join(RECEIPTS)).map_err(|e| e.to_string())? {
            let entry = entry.map_err(|e| e.to_string())?;
            fs::copy(entry.path(), receipts.join(entry.file_name())).map_err(|e| e.to_string())?;
        }
        copy_lane_baselines(&real, &scratch)?;
        assert_eq!(
            render(&scratch)?,
            fs::read_to_string(real.join(PAGE)).map_err(|e| e.to_string())?,
            "valid receipts must preserve the committed page exactly"
        );
        test(&scratch)
    })();
    fs::remove_dir_all(&scratch).map_err(|e| e.to_string())?;
    result
}

fn malformed_strings() -> Vec<Option<Value>> {
    vec![
        None,
        Some(Value::Null),
        Some(serde_json::json!(false)),
        Some(serde_json::json!(42)),
        Some(serde_json::json!([])),
        Some(serde_json::json!({})),
    ]
}

fn replace_required(
    value: &mut Value,
    key: &str,
    replacement: &Option<Value>,
) -> Result<(), String> {
    let object = value.as_object_mut().ok_or("fixture is not an object")?;
    assert!(object.remove(key).is_some(), "fixture lost {key}");
    if let Some(replacement) = replacement {
        object.insert(key.to_string(), replacement.clone());
    }
    Ok(())
}

fn write_json(path: &Path, value: &Value) -> Result<(), String> {
    fs::write(path, serde_json::to_vec(value).map_err(|e| e.to_string())?)
        .map_err(|e| e.to_string())
}

fn check_metric_metadata(root: &Path, path: &str, id: &str) -> Result<(), String> {
    let path = root.join(path);
    let original = read_json(&path)?;
    let valid_page = render(root)?;
    for key in ["direction", "unit", "title"] {
        for replacement in malformed_strings() {
            let mut receipt = original.clone();
            let metric = receipt["metrics"]
                .as_array_mut()
                .ok_or("fixture lost metrics")?
                .iter_mut()
                .find(|metric| field(metric, "id").as_str() == Some(id))
                .ok_or_else(|| format!("fixture lost {id}"))?;
            replace_required(metric, key, &replacement)?;
            write_json(&path, &receipt)?;
            let error = render(root)
                .err()
                .ok_or_else(|| format!("{id} accepted {key}={replacement:?}"))?;
            assert_eq!(error, format!("{id}: missing string `{key}`"));
        }
    }
    write_json(&path, &original)?;
    assert_eq!(
        render(root)?,
        valid_page,
        "restoring valid receipts changed output"
    );
    Ok(())
}

#[test]
fn generator_rejects_missing_or_malformed_scoreboard_metadata() -> Result<(), String> {
    with_receipt_copy(|root| {
        // Exercise both direction conventions and a derived measurement.
        for id in ["ci.workflow_lines", "trust.discriminator_claim_agreement"] {
            check_metric_metadata(root, "metrics/public-proof/dx-scoreboard.json", id)?;
        }
        Ok(())
    })
}

#[test]
fn generator_rejects_missing_or_malformed_selected_lane_metadata() -> Result<(), String> {
    with_receipt_copy(|root| {
        for (path, id) in [
            (CORPUS_FULL_BASELINE, "corpus.not_analyzed"),
            (CORPUS_FAST_BASELINE, "corpus.not_analyzed"),
            (RANKING_BASELINE, "ranking.pilot_precision_top5"),
        ] {
            if path == CORPUS_FAST_BASELINE {
                fs::remove_file(root.join(CORPUS_FULL_BASELINE)).map_err(|e| e.to_string())?;
            }
            let receipts = load(root)?;
            let selected = bars(&receipts)?
                .into_iter()
                .find(|bar| bar.id == id)
                .ok_or("fixture lost lane bar")?;
            assert_eq!(
                selected.basis, path,
                "test did not select the intended lane"
            );
            check_metric_metadata(root, path, id)?;
        }
        Ok(())
    })
}

#[test]
fn generator_and_bars_reject_missing_or_malformed_first_run_cases() -> Result<(), String> {
    with_receipt_copy(|root| {
        for name in ["first-run-current", "first-run-previous"] {
            let path = root.join(RECEIPTS).join(format!("{name}.json"));
            let original = read_json(&path)?;
            let valid_page = render(root)?;
            for replacement in [
                None,
                Some(Value::Null),
                Some(serde_json::json!(false)),
                Some(serde_json::json!(42)),
                Some(serde_json::json!("cases")),
                Some(serde_json::json!({})),
            ] {
                let mut receipt = original.clone();
                replace_required(&mut receipt, "cases", &replacement)?;
                write_json(&path, &receipt)?;
                let expected = format!("{name}: missing array `cases`");
                assert_eq!(render(root).err(), Some(expected.clone()));
                // The complete renderer already rejected cases via its header;
                // the bars path must also refuse rather than derive zero counts.
                assert_eq!(bars(&load(root)?).err(), Some(expected));
            }
            write_json(&path, &original)?;
            assert_eq!(render(root)?, valid_page);
        }
        Ok(())
    })
}
