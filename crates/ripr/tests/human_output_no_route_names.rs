//! #15309: human output is plain words. An internal analyzer route
//! identifier (`analysis/<slug>`) stays in structured JSON fields and never
//! reaches the default human report. The full human report may name it only
//! on its labeled `analyzer route:` diagnostic line, never in next-step prose.

use std::path::{Path, PathBuf};

fn fixtures_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../fixtures")
}

#[test]
fn committed_human_goldens_name_no_internal_analyzer_route() -> Result<(), String> {
    let mut checked = 0usize;
    let mut leaks = Vec::new();
    for entry in std::fs::read_dir(fixtures_dir()).map_err(|e| e.to_string())? {
        let expected = entry.map_err(|e| e.to_string())?.path().join("expected");
        for name in ["human.txt", "human-full.txt"] {
            let Ok(text) = std::fs::read_to_string(expected.join(name)) else {
                continue;
            };
            checked += 1;
            // Wrapped lines can split the phrase, so test the joined text.
            let joined = text.split_whitespace().collect::<Vec<_>>().join(" ");
            let mut needles = vec!["Route analyzer work to", " `analysis/"];
            if name == "human.txt" {
                // The default report names no route identifier at all.
                needles.push(" analysis/");
            }
            for needle in needles {
                if joined.contains(needle) {
                    leaks.push(format!("{}: {needle}", expected.join(name).display()));
                }
            }
        }
    }
    if checked < 100 {
        return Err(format!("expected many human goldens, found {checked}"));
    }
    if leaks.is_empty() {
        Ok(())
    } else {
        Err(leaks.join("\n"))
    }
}
