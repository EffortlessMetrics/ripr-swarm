//! The memoized mask equals the scan for every text, including when long
//! texts alternate on one thread past the memo's capacity, and every seam
//! of a long owner shares one scan of its body (#15189).

use super::super::scan_rust_comments_and_strings;
use super::{MEMO_ENTRIES, MEMO_MIN_BYTES, masked};
use std::cell::Cell;

/// Text fragments covering every masked form and near misses.
const PIECES: &[&str] = &[
    "if a == 1 { 1 } else ",
    "// line comment\n",
    "/* block /* nested */ still */",
    "\"a string with // and /* inside\"",
    "r#\"raw \" string\"#",
    "'\"'",
    "'a ",
    "b'x'",
    "let v = amount;\n",
    "\n",
    "x",
];

fn text_from(choices: &[u8], repeat: usize) -> String {
    let unit: String = choices
        .iter()
        .map(|choice| PIECES[usize::from(*choice) % PIECES.len()])
        .collect();
    unit.repeat(repeat)
}

thread_local! {
    static SCANS: Cell<usize> = const { Cell::new(0) };
}

fn counting_scan(text: &str) -> String {
    SCANS.with(|scans| scans.set(scans.get() + 1));
    scan_rust_comments_and_strings(text)
}

#[test]
fn memoized_mask_equals_the_scan_short_and_long_interleaved() -> Result<(), String> {
    let mut texts = Vec::new();
    for seed in 0u8..24 {
        let choices: Vec<u8> = (0..7)
            .map(|step| seed.wrapping_mul(7).wrapping_add(step * 3))
            .collect();
        // Short texts bypass the memo; long ones (repeated) enter it.
        texts.push(text_from(&choices, 1));
        texts.push(text_from(&choices, 64));
    }
    if !texts.iter().any(|text| text.len() < MEMO_MIN_BYTES)
        || texts
            .iter()
            .filter(|text| text.len() >= MEMO_MIN_BYTES)
            .count()
            <= MEMO_ENTRIES
    {
        return Err("fixture must hold short texts and more long texts than the memo keeps".into());
    }
    // Three passes in alternating order: misses, hits, and evicted entries.
    for round in 0..3 {
        let order: Vec<&String> = if round == 1 {
            texts.iter().rev().collect()
        } else {
            texts.iter().collect()
        };
        for text in order {
            let expected = scan_rust_comments_and_strings(text);
            let first = masked(text, scan_rust_comments_and_strings);
            let again = masked(text, scan_rust_comments_and_strings);
            if first != expected || again != expected {
                return Err(format!("memoized mask differs from the scan for {text:?}"));
            }
        }
    }
    Ok(())
}

#[test]
fn every_seam_of_a_long_owner_shares_one_scan_of_its_body() -> Result<(), String> {
    const SEAMS: usize = 1_900;
    let mut body = String::from("    if a == 0 { 0 }");
    for arm in 1..SEAMS {
        body.push_str(&format!(" else if a == {arm} {{ {arm} }}"));
    }
    let line = body.clone() + " else { a }";
    SCANS.with(|scans| scans.set(0));
    for _seam in 0..SEAMS {
        // Each seam asks about the body and the line, as the evidence pass does.
        let masked_body = masked(&body, counting_scan);
        let masked_line = masked(&line, counting_scan);
        if masked_body.len() != body.len() || masked_line.len() != line.len() {
            return Err("mask changed a byte offset".into());
        }
    }
    let scans = SCANS.with(Cell::get);
    if scans != 2 {
        return Err(format!(
            "{SEAMS} seams over two texts scanned {scans} times, not 2"
        ));
    }
    // A short text is never memoized: it is scanned on every call.
    SCANS.with(|scans| scans.set(0));
    for _call in 0..3 {
        let _ = masked("let v = amount; // note", counting_scan);
    }
    let short_scans = SCANS.with(Cell::get);
    if short_scans != 3 {
        return Err(format!("short text scanned {short_scans} times, not 3"));
    }
    Ok(())
}
