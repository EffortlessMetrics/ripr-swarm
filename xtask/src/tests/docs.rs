use super::*;

#[test]
fn doc_artifacts_rejects_duplicate_artifact_id() -> Result<(), String> {
    with_temp_cwd("doc-artifacts-duplicate", |root| {
        write_doc_artifact_fixture(
            root,
            "docs/proposals/RIPR-PROP-0001-alpha.md",
            "RIPR-PROP-0001",
        );
        write_doc_artifact_fixture(
            root,
            "docs/proposals/RIPR-PROP-0001-beta.md",
            "RIPR-PROP-0001",
        );
        write_doc_artifact_ledger_fixture(
            root,
            r#"schema_version = "1.0"

[[artifact]]
id = "RIPR-PROP-0001"
kind = "proposal"
path = "docs/proposals/RIPR-PROP-0001-alpha.md"
status = "proposed"
owner = "repo-infra"

[[artifact]]
id = "RIPR-PROP-0001"
kind = "proposal"
path = "docs/proposals/RIPR-PROP-0001-beta.md"
status = "proposed"
owner = "repo-infra"
"#,
        );

        let violations = doc_artifact_violations(root, &root.join(DOC_ARTIFACT_LEDGER))?;
        assert!(
            violations
                .iter()
                .any(|violation| violation.contains("duplicate artifact id `RIPR-PROP-0001`")),
            "{violations:#?}"
        );
        Ok(())
    })
}

#[test]
fn doc_artifacts_rejects_duplicate_artifact_field() {
    let error = parse_doc_artifact_ledger_text(
        "policy/doc-artifacts.toml",
        r#"schema_version = "1.0"

[[artifact]]
id = "RIPR-PROP-0001"
id = "RIPR-PROP-0002"
kind = "proposal"
path = "docs/proposals/RIPR-PROP-0001-alpha.md"
status = "proposed"
owner = "repo-infra"
"#,
    )
    .expect_err("duplicate keys should be rejected");

    assert!(error.contains("duplicate field `id`"), "{error}");
}

#[test]
fn doc_artifacts_rejects_missing_artifact_file() -> Result<(), String> {
    with_temp_cwd("doc-artifacts-missing-file", |root| {
        write_doc_artifact_ledger_fixture(
            root,
            r#"schema_version = "1.0"

[[artifact]]
id = "RIPR-PROP-0001"
kind = "proposal"
path = "docs/proposals/RIPR-PROP-0001-missing.md"
status = "proposed"
owner = "repo-infra"
"#,
        );

        let violations = doc_artifact_violations(root, &root.join(DOC_ARTIFACT_LEDGER))?;
        assert!(
            violations
                .iter()
                .any(|violation| violation.contains("points at missing file")),
            "{violations:#?}"
        );
        Ok(())
    })
}

#[test]
fn doc_artifacts_rejects_unknown_status() -> Result<(), String> {
    with_temp_cwd("doc-artifacts-unknown-status", |root| {
        write_doc_artifact_fixture(
            root,
            "docs/proposals/RIPR-PROP-0001-alpha.md",
            "RIPR-PROP-0001",
        );
        write_doc_artifact_ledger_fixture(
            root,
            r#"schema_version = "1.0"

[[artifact]]
id = "RIPR-PROP-0001"
kind = "proposal"
path = "docs/proposals/RIPR-PROP-0001-alpha.md"
status = "maybe"
owner = "repo-infra"
"#,
        );

        let violations = doc_artifact_violations(root, &root.join(DOC_ARTIFACT_LEDGER))?;
        assert!(
            violations
                .iter()
                .any(|violation| violation.contains("unsupported status `maybe`")),
            "{violations:#?}"
        );
        Ok(())
    })
}

#[test]
fn doc_artifacts_rejects_missing_linked_proposal() -> Result<(), String> {
    with_temp_cwd("doc-artifacts-missing-linked-proposal", |root| {
        write_doc_artifact_fixture(
            root,
            "docs/specs/RIPR-SPEC-0001-source-of-truth.md",
            "RIPR-SPEC-0001",
        );
        write_doc_artifact_ledger_fixture(
            root,
            r#"schema_version = "1.0"

[[artifact]]
id = "RIPR-SPEC-0001"
kind = "spec"
path = "docs/specs/RIPR-SPEC-0001-source-of-truth.md"
status = "accepted"
owner = "repo-infra"
"#,
        );

        let violations = doc_artifact_violations(root, &root.join(DOC_ARTIFACT_LEDGER))?;
        assert!(
            violations.iter().any(|violation| {
                violation.contains("must set linked_proposal or standalone_reason")
            }),
            "{violations:#?}"
        );
        Ok(())
    })
}

#[test]
fn doc_artifacts_accepts_superseded_with_replacement() -> Result<(), String> {
    with_temp_cwd("doc-artifacts-superseded", |root| {
        write_doc_artifact_fixture(
            root,
            "docs/proposals/RIPR-PROP-0001-old.md",
            "RIPR-PROP-0001",
        );
        write_doc_artifact_fixture(
            root,
            "docs/proposals/RIPR-PROP-0002-new.md",
            "RIPR-PROP-0002",
        );
        write_doc_artifact_ledger_fixture(
            root,
            r#"schema_version = "1.0"

[[artifact]]
id = "RIPR-PROP-0001"
kind = "proposal"
path = "docs/proposals/RIPR-PROP-0001-old.md"
status = "superseded"
owner = "repo-infra"
superseded_by = "RIPR-PROP-0002"

[[artifact]]
id = "RIPR-PROP-0002"
kind = "proposal"
path = "docs/proposals/RIPR-PROP-0002-new.md"
status = "accepted"
owner = "repo-infra"
"#,
        );

        let violations = doc_artifact_violations(root, &root.join(DOC_ARTIFACT_LEDGER))?;
        assert_eq!(violations, Vec::<String>::new());
        Ok(())
    })
}

#[test]
fn doc_artifacts_rejects_superseded_self_replacement() -> Result<(), String> {
    with_temp_cwd("doc-artifacts-superseded-self", |root| {
        write_doc_artifact_fixture(
            root,
            "docs/proposals/RIPR-PROP-0001-old.md",
            "RIPR-PROP-0001",
        );
        write_doc_artifact_ledger_fixture(
            root,
            r#"schema_version = "1.0"

[[artifact]]
id = "RIPR-PROP-0001"
kind = "proposal"
path = "docs/proposals/RIPR-PROP-0001-old.md"
status = "superseded"
owner = "repo-infra"
superseded_by = "RIPR-PROP-0001"
"#,
        );

        let violations = doc_artifact_violations(root, &root.join(DOC_ARTIFACT_LEDGER))?;
        assert!(
            violations
                .iter()
                .any(|violation| violation.contains("must not replace itself")),
            "{violations:#?}"
        );
        Ok(())
    })
}

#[test]
fn doc_artifacts_rejects_bad_schema_version() -> Result<(), String> {
    with_temp_cwd("doc-artifacts-bad-schema", |root| {
        write_doc_artifact_fixture(
            root,
            "docs/proposals/RIPR-PROP-0001-alpha.md",
            "RIPR-PROP-0001",
        );
        write_doc_artifact_ledger_fixture(
            root,
            r#"schema_version = "0.9"

[[artifact]]
id = "RIPR-PROP-0001"
kind = "proposal"
path = "docs/proposals/RIPR-PROP-0001-alpha.md"
status = "proposed"
owner = "repo-infra"
"#,
        );

        let violations = doc_artifact_violations(root, &root.join(DOC_ARTIFACT_LEDGER))?;
        assert!(
            violations
                .iter()
                .any(|violation| violation.contains("schema_version must be 1.0")),
            "{violations:#?}"
        );
        Ok(())
    })
}

#[test]
fn doc_artifacts_rejects_id_missing_from_file() -> Result<(), String> {
    with_temp_cwd("doc-artifacts-id-missing-from-file", |root| {
        write_doc_artifact_fixture(
            root,
            "docs/proposals/RIPR-PROP-0001-alpha.md",
            "Different ID",
        );
        write_doc_artifact_ledger_fixture(
            root,
            r#"schema_version = "1.0"

[[artifact]]
id = "RIPR-PROP-0001"
kind = "proposal"
path = "docs/proposals/RIPR-PROP-0001-alpha.md"
status = "proposed"
owner = "repo-infra"
"#,
        );

        let violations = doc_artifact_violations(root, &root.join(DOC_ARTIFACT_LEDGER))?;
        assert!(
            violations
                .iter()
                .any(|violation| violation.contains("does not mention `RIPR-PROP-0001`")),
            "{violations:#?}"
        );
        Ok(())
    })
}

#[test]
fn doc_artifacts_rejects_kind_path_mismatch() -> Result<(), String> {
    with_temp_cwd("doc-artifacts-kind-path", |root| {
        write_doc_artifact_fixture(
            root,
            "docs/specs/RIPR-SPEC-0001-source-of-truth.md",
            "RIPR-PROP-0001",
        );
        write_doc_artifact_ledger_fixture(
            root,
            r#"schema_version = "1.0"

[[artifact]]
id = "RIPR-PROP-0001"
kind = "proposal"
path = "docs/specs/RIPR-SPEC-0001-source-of-truth.md"
status = "proposed"
owner = "repo-infra"
"#,
        );

        let violations = doc_artifact_violations(root, &root.join(DOC_ARTIFACT_LEDGER))?;
        assert!(
            violations
                .iter()
                .any(|violation| violation.contains("kind `proposal` does not match path")),
            "{violations:#?}"
        );
        Ok(())
    })
}

#[test]
fn doc_artifacts_rejects_unknown_linked_proposal() -> Result<(), String> {
    with_temp_cwd("doc-artifacts-unknown-link", |root| {
        write_doc_artifact_fixture(
            root,
            "docs/specs/RIPR-SPEC-0001-source-of-truth.md",
            "RIPR-SPEC-0001",
        );
        write_doc_artifact_ledger_fixture(
            root,
            r#"schema_version = "1.0"

[[artifact]]
id = "RIPR-SPEC-0001"
kind = "spec"
path = "docs/specs/RIPR-SPEC-0001-source-of-truth.md"
status = "accepted"
owner = "repo-infra"
linked_proposal = "RIPR-PROP-9999"
"#,
        );

        let violations = doc_artifact_violations(root, &root.join(DOC_ARTIFACT_LEDGER))?;
        assert!(
            violations.iter().any(|violation| {
                violation.contains("linked_proposal references unknown artifact `RIPR-PROP-9999`")
            }),
            "{violations:#?}"
        );
        Ok(())
    })
}

#[test]
fn doc_artifacts_rejects_unknown_linked_plan() -> Result<(), String> {
    with_temp_cwd("doc-artifacts-unknown-plan-link", |root| {
        write_doc_artifact_fixture(
            root,
            "docs/specs/RIPR-SPEC-0001-source-of-truth.md",
            "RIPR-SPEC-0001",
        );
        write_doc_artifact_ledger_fixture(
            root,
            r#"schema_version = "1.0"

[[artifact]]
id = "RIPR-SPEC-0001"
kind = "spec"
path = "docs/specs/RIPR-SPEC-0001-source-of-truth.md"
status = "accepted"
owner = "repo-infra"
standalone_reason = "Repository invariant without a separate proposal."
linked_plan = "PLAN-9999"
"#,
        );

        let violations = doc_artifact_violations(root, &root.join(DOC_ARTIFACT_LEDGER))?;
        assert!(
            violations.iter().any(|violation| {
                violation.contains("linked_plan references unknown artifact `PLAN-9999`")
            }),
            "{violations:#?}"
        );
        Ok(())
    })
}

#[test]
fn doc_artifacts_rejects_path_traversal_before_kind_match() -> Result<(), String> {
    with_temp_cwd("doc-artifacts-path-traversal", |root| {
        write_doc_artifact_fixture(
            root,
            "docs/proposals/RIPR-SPEC-0001-source-of-truth.md",
            "RIPR-SPEC-0001",
        );
        write_doc_artifact_ledger_fixture(
            root,
            r#"schema_version = "1.0"

[[artifact]]
id = "RIPR-SPEC-0001"
kind = "spec"
path = "docs/specs/../proposals/RIPR-SPEC-0001-source-of-truth.md"
status = "accepted"
owner = "repo-infra"
standalone_reason = "Repository invariant without a separate proposal."
"#,
        );

        let violations = doc_artifact_violations(root, &root.join(DOC_ARTIFACT_LEDGER))?;
        assert!(
            violations
                .iter()
                .any(|violation| violation.contains("must not contain `..` traversal")),
            "{violations:#?}"
        );
        Ok(())
    })
}

#[test]
fn doc_artifacts_accepts_accepted_spec_with_standalone_reason() -> Result<(), String> {
    with_temp_cwd("doc-artifacts-standalone-spec", |root| {
        write_doc_artifact_fixture(
            root,
            "docs/specs/RIPR-SPEC-0001-source-of-truth.md",
            "RIPR-SPEC-0001",
        );
        write_doc_artifact_ledger_fixture(
            root,
            r#"schema_version = "1.0"

[[artifact]]
id = "RIPR-SPEC-0001"
kind = "spec"
path = "docs/specs/RIPR-SPEC-0001-source-of-truth.md"
status = "accepted"
owner = "repo-infra"
standalone_reason = "Repository invariant without a separate proposal."
"#,
        );

        let violations = doc_artifact_violations(root, &root.join(DOC_ARTIFACT_LEDGER))?;
        assert_eq!(violations, Vec::<String>::new());
        Ok(())
    })
}

#[test]
fn doc_artifacts_rejects_superseded_without_replacement() -> Result<(), String> {
    with_temp_cwd("doc-artifacts-superseded-missing", |root| {
        write_doc_artifact_fixture(
            root,
            "docs/proposals/RIPR-PROP-0001-old.md",
            "RIPR-PROP-0001",
        );
        write_doc_artifact_ledger_fixture(
            root,
            r#"schema_version = "1.0"

[[artifact]]
id = "RIPR-PROP-0001"
kind = "proposal"
path = "docs/proposals/RIPR-PROP-0001-old.md"
status = "superseded"
owner = "repo-infra"
"#,
        );

        let violations = doc_artifact_violations(root, &root.join(DOC_ARTIFACT_LEDGER))?;
        assert!(
            violations
                .iter()
                .any(|violation| violation.contains("must set superseded_by or replacement")),
            "{violations:#?}"
        );
        Ok(())
    })
}

#[test]
fn doc_artifacts_rejects_unknown_superseded_replacement() -> Result<(), String> {
    with_temp_cwd("doc-artifacts-superseded-unknown", |root| {
        write_doc_artifact_fixture(
            root,
            "docs/proposals/RIPR-PROP-0001-old.md",
            "RIPR-PROP-0001",
        );
        write_doc_artifact_ledger_fixture(
            root,
            r#"schema_version = "1.0"

[[artifact]]
id = "RIPR-PROP-0001"
kind = "proposal"
path = "docs/proposals/RIPR-PROP-0001-old.md"
status = "superseded"
owner = "repo-infra"
replacement = "RIPR-PROP-9999"
"#,
        );

        let violations = doc_artifact_violations(root, &root.join(DOC_ARTIFACT_LEDGER))?;
        assert!(
            violations.iter().any(|violation| {
                violation.contains("superseded_by references unknown artifact `RIPR-PROP-9999`")
            }),
            "{violations:#?}"
        );
        Ok(())
    })
}

#[test]
fn doc_artifacts_rejects_active_plan_without_proposal_or_spec() -> Result<(), String> {
    with_temp_cwd("doc-artifacts-active-plan-missing-link", |root| {
        write_doc_artifact_fixture(
            root,
            "plans/source-of-truth/implementation-plan.md",
            "RIPR-PLAN-0001",
        );
        write_doc_artifact_ledger_fixture(
            root,
            r#"schema_version = "1.0"

[[artifact]]
id = "RIPR-PLAN-0001"
kind = "plan"
path = "plans/source-of-truth/implementation-plan.md"
status = "active"
owner = "repo-infra"
"#,
        );

        let violations = doc_artifact_violations(root, &root.join(DOC_ARTIFACT_LEDGER))?;
        assert!(
            violations.iter().any(|violation| {
                violation.contains("must link to at least one proposal or spec")
            }),
            "{violations:#?}"
        );
        Ok(())
    })
}

#[test]
fn doc_artifacts_accepts_active_plan_with_spec_link() -> Result<(), String> {
    with_temp_cwd("doc-artifacts-active-plan", |root| {
        write_doc_artifact_fixture(
            root,
            "docs/specs/RIPR-SPEC-0001-source-of-truth.md",
            "RIPR-SPEC-0001",
        );
        write_doc_artifact_fixture(
            root,
            "plans/source-of-truth/implementation-plan.md",
            "RIPR-PLAN-0001",
        );
        write_doc_artifact_ledger_fixture(
            root,
            r#"schema_version = "1.0"

[[artifact]]
id = "RIPR-SPEC-0001"
kind = "spec"
path = "docs/specs/RIPR-SPEC-0001-source-of-truth.md"
status = "accepted"
owner = "repo-infra"
standalone_reason = "Repository invariant without a separate proposal."

[[artifact]]
id = "RIPR-PLAN-0001"
kind = "plan"
path = "plans/source-of-truth/implementation-plan.md"
status = "active"
owner = "repo-infra"
linked_spec = "RIPR-SPEC-0001"
"#,
        );

        let violations = doc_artifact_violations(root, &root.join(DOC_ARTIFACT_LEDGER))?;
        assert_eq!(violations, Vec::<String>::new());
        Ok(())
    })
}

#[test]
fn doc_artifacts_rejects_unknown_artifact_field() {
    with_temp_cwd("doc-artifacts-unknown-field", |root| {
        write_doc_artifact_ledger_fixture(
            root,
            r#"schema_version = "1.0"

[[artifact]]
id = "RIPR-PROP-0001"
kind = "proposal"
path = "docs/proposals/RIPR-PROP-0001-alpha.md"
status = "proposed"
owner = "repo-infra"
surprise = "nope"
"#,
        );

        let error = doc_artifact_violations(root, &root.join(DOC_ARTIFACT_LEDGER))
            .expect_err("unknown fields should be parse errors");
        assert!(error.contains("unknown field `surprise`"));
    });
}

#[test]
fn doc_artifacts_rejects_unsupported_toml_section() {
    with_temp_cwd("doc-artifacts-unsupported-section", |root| {
        write_doc_artifact_ledger_fixture(
            root,
            r#"schema_version = "1.0"

[metadata]
owner = "repo-infra"
"#,
        );

        let error = doc_artifact_violations(root, &root.join(DOC_ARTIFACT_LEDGER))
            .expect_err("unsupported sections should be parse errors");
        assert!(error.contains("unsupported TOML section `[metadata]`"));
    });
}

#[test]
fn doc_artifacts_rejects_unquoted_values() {
    with_temp_cwd("doc-artifacts-unquoted", |root| {
        write_doc_artifact_ledger_fixture(
            root,
            r#"schema_version = "1.0"

[[artifact]]
id = RIPR-PROP-0001
"#,
        );

        let error = doc_artifact_violations(root, &root.join(DOC_ARTIFACT_LEDGER))
            .expect_err("unquoted values should be parse errors");
        assert!(error.contains("string value must be quoted"));
    });
}

#[test]
fn doc_artifacts_command_accepts_valid_fixture() -> Result<(), String> {
    with_temp_cwd("doc-artifacts-command-valid", |root| {
        write_doc_artifact_fixture(
            root,
            "docs/proposals/RIPR-PROP-0001-alpha.md",
            "RIPR-PROP-0001",
        );
        write_doc_artifact_ledger_fixture(
            root,
            r#"schema_version = "1.0"

[[artifact]]
id = "RIPR-PROP-0001"
kind = "proposal"
path = "docs/proposals/RIPR-PROP-0001-alpha.md"
status = "accepted"
owner = "repo-infra"
"#,
        );

        check_doc_artifacts()
    })
}

#[test]
fn doc_artifacts_command_names_unregistered_spec_ids() {
    with_temp_cwd("doc-artifacts-reverse-direction-names-ids", |root| {
        write_doc_artifact_fixture(
            root,
            "docs/proposals/RIPR-PROP-0001-alpha.md",
            "RIPR-PROP-0001",
        );
        write_doc_artifact_fixture(
            root,
            "docs/specs/RIPR-SPEC-1234-widget.md",
            "RIPR-SPEC-1234",
        );
        write_doc_artifact_fixture(
            root,
            "docs/specs/RIPR-SPEC-1235-gadget.md",
            "RIPR-SPEC-1235",
        );
        write_doc_artifact_ledger_fixture(
            root,
            r#"schema_version = "1.0"

[[artifact]]
id = "RIPR-PROP-0001"
kind = "proposal"
path = "docs/proposals/RIPR-PROP-0001-alpha.md"
status = "accepted"
owner = "repo-infra"
"#,
        );

        let error = check_doc_artifacts_with_unregistered_baseline(1)
            .expect_err("two unregistered specs exceed baseline 1");
        assert!(
            error.contains("RIPR-SPEC-1234 (docs/specs/RIPR-SPEC-1234-widget.md)"),
            "command path must name the first unregistered spec, got {error}"
        );
        assert!(
            error.contains("RIPR-SPEC-1235 (docs/specs/RIPR-SPEC-1235-gadget.md)"),
            "command path must name the second unregistered spec, got {error}"
        );
        assert!(
            error.contains("unregistered:"),
            "command path must not keep the count-only reverse-direction message, got {error}"
        );
    });
}

#[test]
fn doc_artifacts_command_rejects_invalid_fixture() {
    with_temp_cwd("doc-artifacts-command-invalid", |root| {
        write_doc_artifact_ledger_fixture(root, "schema_version = \"1.0\"\n");

        let error = check_doc_artifacts().expect_err("empty ledgers should fail the command");
        assert!(error.contains("check-doc-artifacts failed"));
        assert!(error.contains("has no [[artifact]] entries"));
    });
}

#[test]
fn doc_artifacts_command_reports_missing_ledger() {
    with_temp_cwd("doc-artifacts-command-missing-ledger", |_root| {
        let error = check_doc_artifacts().expect_err("missing ledgers should fail the command");
        assert!(error.contains("failed to read policy/doc-artifacts.toml"));
    });
}

#[test]
fn doc_artifacts_rejects_missing_schema_and_empty_ledger() -> Result<(), String> {
    with_temp_cwd("doc-artifacts-empty", |root| {
        write_doc_artifact_ledger_fixture(root, "");

        let violations = doc_artifact_violations(root, &root.join(DOC_ARTIFACT_LEDGER))?;
        assert!(
            violations
                .iter()
                .any(|violation| violation.contains("is missing schema_version")),
            "{violations:#?}"
        );
        assert!(
            violations
                .iter()
                .any(|violation| violation.contains("has no [[artifact]] entries")),
            "{violations:#?}"
        );
        Ok(())
    })
}

#[test]
fn doc_artifacts_rejects_missing_artifact_id() -> Result<(), String> {
    with_temp_cwd("doc-artifacts-missing-id", |root| {
        write_doc_artifact_fixture(
            root,
            "docs/proposals/RIPR-PROP-0001-alpha.md",
            "RIPR-PROP-0001",
        );
        write_doc_artifact_ledger_fixture(
            root,
            r#"schema_version = "1.0"

[[artifact]]
kind = "proposal"
path = "docs/proposals/RIPR-PROP-0001-alpha.md"
status = "proposed"
owner = "repo-infra"
"#,
        );

        let violations = doc_artifact_violations(root, &root.join(DOC_ARTIFACT_LEDGER))?;
        assert!(
            violations
                .iter()
                .any(|violation| violation.contains("artifact is missing required field `id`")),
            "{violations:#?}"
        );
        Ok(())
    })
}

#[test]
fn doc_artifacts_rejects_missing_required_fields() -> Result<(), String> {
    with_temp_cwd("doc-artifacts-missing-required", |root| {
        write_doc_artifact_ledger_fixture(
            root,
            r#"schema_version = "1.0"

[[artifact]]
id = "RIPR-PROP-0001"
"#,
        );

        let violations = doc_artifact_violations(root, &root.join(DOC_ARTIFACT_LEDGER))?;
        assert!(
            violations
                .iter()
                .any(|violation| violation.contains("missing required field `kind`")),
            "{violations:#?}"
        );
        Ok(())
    })
}

#[test]
fn doc_artifacts_rejects_missing_path_after_kind() -> Result<(), String> {
    with_temp_cwd("doc-artifacts-missing-path", |root| {
        write_doc_artifact_ledger_fixture(
            root,
            r#"schema_version = "1.0"

[[artifact]]
id = "RIPR-PROP-0001"
kind = "proposal"
"#,
        );

        let violations = doc_artifact_violations(root, &root.join(DOC_ARTIFACT_LEDGER))?;
        assert!(
            violations
                .iter()
                .any(|violation| violation.contains("missing required field `path`")),
            "{violations:#?}"
        );
        Ok(())
    })
}

#[test]
fn doc_artifacts_rejects_missing_status_after_path() -> Result<(), String> {
    with_temp_cwd("doc-artifacts-missing-status", |root| {
        write_doc_artifact_ledger_fixture(
            root,
            r#"schema_version = "1.0"

[[artifact]]
id = "RIPR-PROP-0001"
kind = "proposal"
path = "docs/proposals/RIPR-PROP-0001-alpha.md"
"#,
        );

        let violations = doc_artifact_violations(root, &root.join(DOC_ARTIFACT_LEDGER))?;
        assert!(
            violations
                .iter()
                .any(|violation| violation.contains("missing required field `status`")),
            "{violations:#?}"
        );
        Ok(())
    })
}

#[test]
fn doc_artifacts_rejects_missing_owner() -> Result<(), String> {
    with_temp_cwd("doc-artifacts-missing-owner", |root| {
        write_doc_artifact_fixture(
            root,
            "docs/proposals/RIPR-PROP-0001-alpha.md",
            "RIPR-PROP-0001",
        );
        write_doc_artifact_ledger_fixture(
            root,
            r#"schema_version = "1.0"

[[artifact]]
id = "RIPR-PROP-0001"
kind = "proposal"
path = "docs/proposals/RIPR-PROP-0001-alpha.md"
status = "proposed"
"#,
        );

        let violations = doc_artifact_violations(root, &root.join(DOC_ARTIFACT_LEDGER))?;
        assert!(
            violations
                .iter()
                .any(|violation| violation.contains("missing required field `owner`")),
            "{violations:#?}"
        );
        Ok(())
    })
}

#[test]
fn doc_artifacts_rejects_unknown_kind() -> Result<(), String> {
    with_temp_cwd("doc-artifacts-unknown-kind", |root| {
        write_doc_artifact_fixture(
            root,
            "docs/proposals/RIPR-PROP-0001-alpha.md",
            "RIPR-PROP-0001",
        );
        write_doc_artifact_ledger_fixture(
            root,
            r#"schema_version = "1.0"

[[artifact]]
id = "RIPR-PROP-0001"
kind = "memo"
path = "docs/proposals/RIPR-PROP-0001-alpha.md"
status = "proposed"
owner = "repo-infra"
"#,
        );

        let violations = doc_artifact_violations(root, &root.join(DOC_ARTIFACT_LEDGER))?;
        assert!(
            violations
                .iter()
                .any(|violation| violation.contains("unsupported kind `memo`")),
            "{violations:#?}"
        );
        Ok(())
    })
}

#[test]
fn doc_artifacts_rejects_absolute_path() -> Result<(), String> {
    with_temp_cwd("doc-artifacts-absolute-path", |root| {
        let absolute_path = root
            .join("docs/proposals/RIPR-PROP-0001-alpha.md")
            .display()
            .to_string()
            .replace('\\', "\\\\");
        write_doc_artifact_ledger_fixture(
            root,
            &format!(
                r#"schema_version = "1.0"

[[artifact]]
id = "RIPR-PROP-0001"
kind = "proposal"
path = "{absolute_path}"
status = "proposed"
owner = "repo-infra"
"#
            ),
        );

        let violations = doc_artifact_violations(root, &root.join(DOC_ARTIFACT_LEDGER))?;
        assert!(
            violations
                .iter()
                .any(|violation| violation.contains("path must be repo-relative")),
            "{violations:#?}"
        );
        Ok(())
    })
}

#[test]
fn doc_artifacts_reports_unreadable_artifact_path() -> Result<(), String> {
    with_temp_cwd("doc-artifacts-unreadable-path", |root| {
        fs::create_dir_all(root.join("docs/proposals/RIPR-PROP-0001-alpha.md"))
            .map_err(|err| format!("failed to create fixture directory: {err}"))?;
        write_doc_artifact_ledger_fixture(
            root,
            r#"schema_version = "1.0"

[[artifact]]
id = "RIPR-PROP-0001"
kind = "proposal"
path = "docs/proposals/RIPR-PROP-0001-alpha.md"
status = "proposed"
owner = "repo-infra"
"#,
        );

        let error = doc_artifact_violations(root, &root.join(DOC_ARTIFACT_LEDGER))
            .expect_err("directories should fail artifact reads");
        assert!(error.contains("failed to read"));
        assert!(error.contains("docs/proposals/RIPR-PROP-0001-alpha.md"));
        Ok(())
    })
}

#[test]
fn doc_artifacts_rejects_key_value_before_artifact() {
    with_temp_cwd("doc-artifacts-key-before-artifact", |root| {
        write_doc_artifact_ledger_fixture(
            root,
            r#"schema_version = "1.0"
owner = "repo-infra"
"#,
        );

        let error = doc_artifact_violations(root, &root.join(DOC_ARTIFACT_LEDGER))
            .expect_err("artifact fields before a section should be parse errors");
        assert!(error.contains("`owner` must appear inside [[artifact]]"));
    });
}

#[test]
fn doc_artifacts_rejects_malformed_line() {
    with_temp_cwd("doc-artifacts-malformed-line", |root| {
        write_doc_artifact_ledger_fixture(
            root,
            r#"schema_version = "1.0"

[[artifact]]
not a key value
"#,
        );

        let error = doc_artifact_violations(root, &root.join(DOC_ARTIFACT_LEDGER))
            .expect_err("malformed lines should be parse errors");
        assert!(error.contains("expected `key = \"value\"`"));
    });
}

#[test]
fn doc_artifacts_accepts_adr_links_and_ignored_metadata_fields() -> Result<(), String> {
    with_temp_cwd("doc-artifacts-adr-links", |root| {
        write_doc_artifact_fixture(
            root,
            "docs/proposals/RIPR-PROP-0001-alpha.md",
            "RIPR-PROP-0001",
        );
        write_doc_artifact_fixture(root, "docs/adr/RIPR-ADR-0001-alpha.md", "RIPR-ADR-0001");
        write_doc_artifact_fixture(root, "docs/specs/RIPR-SPEC-0001-alpha.md", "RIPR-SPEC-0001");
        write_doc_artifact_fixture(
            root,
            "plans/source-of-truth/implementation-plan.md",
            "PLAN-0001",
        );
        write_doc_artifact_ledger_fixture(
            root,
            r#"schema_version = "1.0"

[[artifact]]
id = "RIPR-PROP-0001"
kind = "proposal"
path = "docs/proposals/RIPR-PROP-0001-alpha.md"
status = "accepted"
owner = "repo-infra"
notes = "metadata ignored by validator"
reason = "policy context"
review_posture = "advisory"

[[artifact]]
id = "RIPR-ADR-0001"
kind = "adr"
path = "docs/adr/RIPR-ADR-0001-alpha.md"
status = "accepted"
owner = "repo-infra"
linked_proposal = "RIPR-PROP-0001"

[[artifact]]
id = "RIPR-SPEC-0001"
kind = "spec"
path = "docs/specs/RIPR-SPEC-0001-alpha.md"
status = "accepted"
owner = "repo-infra"
linked_proposal = "RIPR-PROP-0001"
linked_adr = "RIPR-ADR-0001"
linked_plan = "PLAN-0001"

[[artifact]]
id = "PLAN-0001"
kind = "plan"
path = "plans/source-of-truth/implementation-plan.md"
status = "active"
owner = "repo-infra"
linked_spec = "RIPR-SPEC-0001"
"#,
        );

        let violations = doc_artifact_violations(root, &root.join(DOC_ARTIFACT_LEDGER))?;
        assert!(violations.is_empty(), "{violations:#?}");
        Ok(())
    })
}

#[test]
fn doc_artifact_kind_path_rules_cover_supported_surfaces() {
    let cases = [
        ("adr", "docs/adr/ADR-0001.md", true),
        ("closeout", "docs/handoffs/2026-closeout.md", true),
        ("closeout", "plans/source-of-truth/closeout.md", true),
        ("goal", ".ripr/goals/active.toml", true),
        ("plan", "plans/source-of-truth/implementation-plan.md", true),
        ("policy-ledger", "policy/doc-artifacts.toml", true),
        ("policy_ledger", "policy/doc-artifacts.toml", true),
        ("support-tier", "docs/status/SUPPORT_TIERS.md", true),
        ("support_tier", "docs/status/SUPPORT_TIERS.md", true),
        ("support_tier", "docs/status/CLAIMS.md", true),
        ("roadmap", "docs/ROADMAP.md", true),
        ("roadmap", "ROADMAP.md", true),
        ("unknown", "docs/ROADMAP.md", false),
    ];

    for (kind, path, expected) in cases {
        assert_eq!(
            doc_artifact_kind_matches_path(kind, path),
            expected,
            "{kind} {path}"
        );
    }
}
