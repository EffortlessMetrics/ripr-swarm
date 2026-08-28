use crate::cli::commands_options::InitCi;
use std::path::{Path, PathBuf};

const CHECKOUT_V6_1_0_SHA: &str = "d23441a48e516b6c34aea4fa41551a30e30af803";
const UPLOAD_ARTIFACT_V7_0_1_SHA: &str = "043fb46d1a93c77aae656e7c1c64a875d1fc6a0a";
const CREATE_PULL_REQUEST_V8_1_1_SHA: &str = "5f697829c57c59eccef0ff001569ddc5afee4fee";

pub(super) fn workflow_path(root: &Path, ci: &InitCi) -> PathBuf {
    match ci {
        InitCi::Github => root.join(".github/workflows/ripr-badge.yml"),
    }
}

pub(super) fn generated_github_badge_workflow() -> String {
    format!(
        r#"name: RIPR badge refresh

# Badge publication is intentionally separate from the advisory PR workflow.
# This workflow opens a narrow PR; it never pushes to the default branch.
on:
  workflow_dispatch:
  schedule:
    - cron: "17 6 * * 1"

permissions:
  contents: write
  pull-requests: write

concurrency:
  group: ripr-badge-refresh
  cancel-in-progress: false

env:
  RIPR_VERSION: "{version}"

jobs:
  refresh:
    runs-on: ubuntu-latest
    timeout-minutes: 30
    steps:
      - name: Check out the default branch
        uses: actions/checkout@{checkout_sha} # v6.1.0
        with:
          ref: ${{{{ github.event.repository.default_branch }}}}
          fetch-depth: 0
          persist-credentials: false

      - name: Install Rust
        run: |
          rustup toolchain install stable --profile minimal --no-self-update
          rustup default stable

      - name: Install pinned RIPR
        run: cargo install ripr --version "=$RIPR_VERSION" --locked

      - name: Render RIPR repo badge artifacts
        run: |
          mkdir -p target/ripr/reports badges
          ripr check --root . --mode ready --format repo-badge-json \
            > target/ripr/reports/repo-ripr-badge.json || true
          ripr check --root . --mode ready --format repo-badge-shields \
            > badges/ripr.json || true

      - name: Validate RIPR badge schemas
        run: |
          jq -e '
            type == "object" and
            .kind == "ripr" and
            .scope == "repo" and
            .basis == "canonical_actionable_gap" and
            (.message | type == "string") and
            (.color | type == "string")
          ' target/ripr/reports/repo-ripr-badge.json > /dev/null

          jq -e '
            type == "object" and
            .schemaVersion == 1 and
            .label == "ripr" and
            (.message | type == "string") and
            (.color | type == "string") and
            ((keys | sort) == ["color", "label", "message", "schemaVersion"])
          ' badges/ripr.json > /dev/null

      - name: Retain the native RIPR audit artifact
        uses: actions/upload-artifact@{upload_artifact_sha} # v7.0.1
        with:
          name: ripr-repo-badge-audit
          path: target/ripr/reports/repo-ripr-badge.json
          if-no-files-found: error
          retention-days: 14

      - name: Open a RIPR badge refresh PR
        uses: peter-evans/create-pull-request@{create_pull_request_sha} # v8.1.1
        with:
          token: ${{{{ github.token }}}}
          branch: automation/ripr-badge-refresh
          delete-branch: true
          commit-message: "chore(badges): refresh RIPR endpoint"
          title: "chore(badges): refresh RIPR endpoint"
          body: |
            Refreshes `badges/ripr.json` from RIPR's repo-scoped static evidence.

            Generated with RIPR {version}. The native audit artifact is retained
            on the workflow run; this PR changes only the public Shields endpoint.
          add-paths: |
            badges/ripr.json
"#,
        version = env!("CARGO_PKG_VERSION"),
        checkout_sha = CHECKOUT_V6_1_0_SHA,
        upload_artifact_sha = UPLOAD_ARTIFACT_V7_0_1_SHA,
        create_pull_request_sha = CREATE_PULL_REQUEST_V8_1_1_SHA,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn init_badge_workflow_targets_a_separate_file() {
        assert_eq!(
            workflow_path(Path::new("repo"), &InitCi::Github),
            PathBuf::from("repo/.github/workflows/ripr-badge.yml")
        );
    }

    #[test]
    fn init_badge_workflow_is_scheduled_pr_scoped_and_contract_validated() {
        let workflow = generated_github_badge_workflow();

        for required in [
            "workflow_dispatch:",
            "schedule:",
            "contents: write",
            "pull-requests: write",
            "persist-credentials: false",
            "cargo install ripr --version \"=$RIPR_VERSION\" --locked",
            "target/ripr/reports/repo-ripr-badge.json",
            "badges/ripr.json",
            "canonical_actionable_gap",
            ".schemaVersion == 1",
            ".label == \"ripr\"",
            "((keys | sort) == [\"color\", \"label\", \"message\", \"schemaVersion\"])",
            "actions/checkout@d23441a48e516b6c34aea4fa41551a30e30af803",
            "actions/upload-artifact@043fb46d1a93c77aae656e7c1c64a875d1fc6a0a",
            "name: ripr-repo-badge-audit",
            "peter-evans/create-pull-request@5f697829c57c59eccef0ff001569ddc5afee4fee",
            "branch: automation/ripr-badge-refresh",
            "add-paths: |\n            badges/ripr.json",
        ] {
            assert!(
                workflow.contains(required),
                "generated badge workflow missing `{required}`"
            );
        }

        assert!(!workflow.contains("\n  pull_request:"));
        assert!(!workflow.contains("\n  push:"));
        assert!(!workflow.contains("git push"));
        assert!(!workflow.contains("archive: false"));
        assert!(workflow.contains(env!("CARGO_PKG_VERSION")));
    }
}
