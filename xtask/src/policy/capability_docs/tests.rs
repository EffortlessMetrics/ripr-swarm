use super::{Mode, block_violations, locate_block, parse_args, replace_block, validate_rendering};

const BEGIN: &str = "<!-- capability-registry:begin (generated; do not edit) -->";
const END: &str = "<!-- capability-registry:end -->";

fn rendering(state: &str) -> String {
    format!(
        "{BEGIN}\n\n| Cell | State | Ceiling |\n| --- | --- | --- |\n| `rust/any/diff/static_facts` | `{state}` | `stable` |\n\n{END}"
    )
}

fn doc_with(block: &str) -> String {
    format!("# Doc\n\nbefore\n\n{block}\n\nafter\n")
}

fn refused<T>(result: Result<T, String>) -> bool {
    result.is_err()
}

fn accepted<T>(result: Result<T, String>) -> bool {
    result.is_ok()
}

fn args(items: &[&str]) -> Vec<String> {
    items.iter().map(|item| (*item).to_string()).collect()
}

#[test]
fn a_current_block_passes() {
    let rendered = rendering("state_a");
    assert_eq!(
        block_violations(
            "doc.md",
            &doc_with(&rendered),
            &validate_rendering(&format!("{rendered}\n")).unwrap_or_default()
        ),
        Vec::<String>::new()
    );
}

#[test]
fn a_stale_block_fails_with_the_write_hint() {
    let violations = block_violations(
        "doc.md",
        &doc_with(&rendering("state_a")),
        &rendering("state_b"),
    );
    assert_eq!(violations.len(), 1, "{violations:?}");
    assert!(
        violations[0].contains("capability-docs --write"),
        "{violations:?}"
    );
}

#[test]
fn missing_duplicate_and_reversed_markers_fail_closed() {
    let rendered = rendering("state_a");
    for doc in [
        "# no markers\n".to_string(),
        format!("{BEGIN}\nno end\n"),
        format!("{END}\n"),
        format!("{}\n{}\n", rendering("a"), rendering("b")),
        format!("{END}\n{BEGIN}\n"),
    ] {
        assert!(refused(locate_block(&doc)), "{doc}");
        assert_eq!(
            block_violations("doc.md", &doc, &rendered).len(),
            1,
            "{doc}"
        );
        assert!(refused(replace_block(&doc, &rendered)), "{doc}");
    }
}

#[test]
fn write_round_trip_rewrites_only_the_block() -> Result<(), String> {
    let stale = doc_with(&rendering("state_b"));
    let fresh = rendering("state_a");
    let written = replace_block(&stale, &fresh)?;
    assert_eq!(written, doc_with(&fresh));
    assert!(written.starts_with("# Doc\n\nbefore\n\n"));
    assert!(written.ends_with("\n\nafter\n"));
    assert_eq!(
        block_violations("doc.md", &written, &fresh),
        Vec::<String>::new()
    );
    assert_eq!(replace_block(&written, &fresh)?, written);
    Ok(())
}

#[test]
fn bad_renderings_are_refused() {
    assert!(refused(validate_rendering("")));
    assert!(refused(validate_rendering("\n\n")));
    assert!(refused(validate_rendering("| `a` | `b` |\n")));
    let no_rows = format!("{BEGIN}\n\n{END}\n");
    assert!(validate_rendering(&no_rows).is_err_and(|e| e.contains("no cell rows")));
    let doubled = format!("{}\n{}", rendering("a"), rendering("b"));
    assert!(refused(validate_rendering(&doubled)));
    assert_eq!(
        validate_rendering(&format!("{}\n", rendering("a"))),
        Ok(rendering("a"))
    );
}

#[test]
fn arguments_default_to_check_and_reject_conflicts() {
    assert_eq!(parse_args(&args(&[])).map(|o| o.mode), Ok(Mode::Check));
    assert_eq!(
        parse_args(&args(&["--write"])).map(|o| o.mode),
        Ok(Mode::Write)
    );
    assert!(refused(parse_args(&args(&["--check", "--write"]))));
    assert!(refused(parse_args(&args(&["--markdown-file"]))));
    assert!(refused(parse_args(&args(&["--bogus"]))));
    assert_eq!(
        parse_args(&args(&["--markdown-file", "x.md"])).map(|o| o.markdown_file),
        Ok(Some("x.md".into()))
    );
}

#[test]
fn every_bound_doc_has_one_locatable_block_and_a_hand_edit_is_caught() -> Result<(), String> {
    assert!(
        super::DOC_PATHS.contains(&"docs/LANGUAGE_ADAPTER_PREVIEW.md"),
        "the preview guide must stay bound to the registry block"
    );
    let mut blocks = Vec::new();
    for rel in super::DOC_PATHS {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("..")
            .join(rel);
        let doc = std::fs::read_to_string(&path).map_err(|e| format!("{}: {e}", path.display()))?;
        let (start, end) = locate_block(&doc)?;
        let block = doc.get(start..end).ok_or("range")?.to_string();
        assert!(
            accepted(validate_rendering(&block)),
            "{rel}: real block is not a valid rendering"
        );
        assert_eq!(block_violations(rel, &doc, &block), Vec::<String>::new());
        // Negative experiment: flip one documented state inside the block only.
        let edited_block = block.replacen("`preview`", "`state_x`", 1);
        let edited_doc = doc.replacen(&block, &edited_block, 1);
        assert_ne!(
            edited_block, block,
            "{rel}: real block has no preview cell to edit"
        );
        let violations = block_violations(rel, &edited_doc, &block);
        assert_eq!(violations.len(), 1, "{rel}: {violations:?}");
        assert!(violations[0].contains(rel), "{violations:?}");
        blocks.push(block);
    }
    assert!(
        blocks.windows(2).all(|pair| pair[0] == pair[1]),
        "bound docs embed the same registry rendering"
    );
    Ok(())
}
