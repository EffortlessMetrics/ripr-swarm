//! Row joins, counts, and terminal disposition derivation for the verification
//! receipt check.
//!
//! Loads the accepted selection manifest and retained receipt files, joins
//! valid receipts, and derives the fail-closed `valid` / `inconsistent` /
//! `not_run` verdict.

use std::path::Path;

use super::super::python_repair_trust::{
    SelectionManifest, load_strict_json, validate_selection_manifest,
};
use super::schema::{VerificationReceipt, verification_fail};
use super::validate::validate_receipt;

pub(crate) struct VerificationCheckOutcome {
    pub(crate) manifest_path: String,
    pub(crate) manifest: Option<SelectionManifest>,
    pub(crate) receipts_input: Option<String>,
    pub(crate) receipts: Vec<VerificationReceipt>,
    pub(crate) violations: Vec<String>,
}

impl VerificationCheckOutcome {
    pub(crate) fn verdict(&self) -> &'static str {
        // Violations take precedence: a set of ALL-invalid receipts has zero
        // valid records but must fail closed as `inconsistent` (nonzero
        // exit), never pass as `not_run`.
        if !self.violations.is_empty() {
            return "inconsistent";
        }
        if self.receipts.is_empty() {
            "not_run"
        } else {
            "valid"
        }
    }
}

pub(crate) fn check_verification_artifacts(
    manifest_path: &str,
    receipts_input: &str,
) -> Result<VerificationCheckOutcome, String> {
    if !Path::new(manifest_path).exists() {
        return Err(verification_fail(
            manifest_path,
            "manifest",
            "verification receipts bind to an accepted selection manifest; no manifest exists at this path",
        ));
    }
    let (value, manifest_sha256) = load_strict_json(manifest_path)?;
    let manifest = validate_selection_manifest(&value, manifest_sha256.clone())?;

    let path = Path::new(receipts_input);
    if !path.exists() {
        return Err(verification_fail(
            receipts_input,
            "receipts",
            "receipts input path does not exist",
        ));
    }
    let files = if path.is_dir() {
        let mut entries = Vec::new();
        let read = std::fs::read_dir(path).map_err(|error| {
            verification_fail(
                receipts_input,
                "receipts",
                format!("failed to read directory: {error}"),
            )
        })?;
        for entry in read {
            let entry = entry.map_err(|error| {
                verification_fail(
                    receipts_input,
                    "receipts",
                    format!("failed to read directory entry: {error}"),
                )
            })?;
            let name = entry.file_name().to_string_lossy().to_string();
            let full = entry.path();
            if full.is_file() && name.ends_with(".json") {
                entries.push(full.to_string_lossy().to_string());
            }
        }
        entries.sort();
        entries
    } else {
        vec![receipts_input.to_string()]
    };

    let mut receipts = Vec::new();
    let mut violations = Vec::new();
    for display in &files {
        match load_strict_json(display) {
            Ok((record, _sha256)) => {
                match validate_receipt(display, &record, &manifest, &manifest_sha256) {
                    Ok(receipt) => receipts.push(receipt),
                    Err(violation) => violations.push(violation),
                }
            }
            Err(error) => violations.push(error),
        }
    }
    Ok(VerificationCheckOutcome {
        manifest_path: manifest_path.to_string(),
        manifest: Some(manifest),
        receipts_input: Some(receipts_input.to_string()),
        receipts,
        violations,
    })
}
