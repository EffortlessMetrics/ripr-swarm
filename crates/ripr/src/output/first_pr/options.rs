use crate::agent::loop_commands::{bound_root, shell_arg};
use std::path::PathBuf;

use super::{
    DEFAULT_AGENT_PACKET, DEFAULT_BASE, DEFAULT_FIRST_ACTION, DEFAULT_GAP_LEDGER,
    DEFAULT_GATE_DECISION, DEFAULT_HEAD, DEFAULT_OUT_DIR, DEFAULT_RECEIPTS_DIR,
    DEFAULT_REVIEW_COMMENTS, DEFAULT_ROOT,
};

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct FirstPrOptions {
    pub(crate) root: String,
    pub(crate) base: String,
    /// `false` when `--base` was omitted: `base` then holds a placeholder
    /// until the CLI entry resolves the repository's default branch.
    pub(crate) base_explicit: bool,
    pub(crate) head: String,
    pub(crate) check_output: Option<String>,
    pub(crate) gap_ledger: String,
    pub(crate) gap_ledger_explicit: bool,
    pub(crate) first_action: String,
    pub(crate) review_comments: String,
    pub(crate) agent_packet: String,
    pub(crate) gate_decision: String,
    pub(crate) receipts_dir: String,
    pub(crate) out_dir: String,
    pub(crate) check: bool,
    pub(crate) preflight: bool,
    /// `--run-chain` (#11720): run the missing-artifact chain before composing.
    pub(crate) run_chain: bool,
    /// Test-only ceiling for `GIT_CEILING_DIRECTORIES`; `None` in production.
    pub(crate) git_ceiling: Option<PathBuf>,
}

impl FirstPrOptions {
    /// The selected root bound once for product-generated commands (#3999):
    /// an existing root follows filesystem traversal, including symlink/`..`.
    /// A relative `--root` resolves against this process's working directory,
    /// the same directory `repo_root` resolved it against, so a pasted command
    /// analyzes and writes the selected repository from any directory.
    pub(super) fn command_root(&self) -> String {
        let root = std::path::Path::new(&self.root);
        root.canonicalize()
            .ok()
            .and_then(|resolved| crate::output::path::command_root_display(root, &resolved).ok())
            .unwrap_or_else(|| bound_root(&self.root))
    }

    /// A first-pr artifact path rendered as a generated command argument,
    /// quoted for the shell. first-pr resolves its artifact paths against the
    /// selected root, while `first-action`, `review-comments`, `agent packet`,
    /// `gate evaluate`, `reports gap-ledger` and shell redirects resolve them
    /// against the invocation working directory. Anchoring at the bound root
    /// keeps `--root` and every path naming the same repository when a command
    /// is pasted elsewhere (#3948, #4287); an absolute path passes through.
    pub(super) fn anchored_arg(&self, path: &str) -> String {
        let path = std::path::Path::new(path);
        let anchored = if path.is_absolute() {
            path.to_path_buf()
        } else {
            PathBuf::from(self.command_root()).join(path)
        };
        shell_arg(&crate::output::path::human_path(&anchored))
    }
}

impl Default for FirstPrOptions {
    fn default() -> Self {
        Self {
            root: DEFAULT_ROOT.to_string(),
            base: DEFAULT_BASE.to_string(),
            base_explicit: false,
            head: DEFAULT_HEAD.to_string(),
            check_output: None,
            gap_ledger: DEFAULT_GAP_LEDGER.to_string(),
            gap_ledger_explicit: false,
            first_action: DEFAULT_FIRST_ACTION.to_string(),
            review_comments: DEFAULT_REVIEW_COMMENTS.to_string(),
            agent_packet: DEFAULT_AGENT_PACKET.to_string(),
            gate_decision: DEFAULT_GATE_DECISION.to_string(),
            receipts_dir: DEFAULT_RECEIPTS_DIR.to_string(),
            out_dir: DEFAULT_OUT_DIR.to_string(),
            check: false,
            preflight: false,
            run_chain: false,
            git_ceiling: None,
        }
    }
}

/// Help body for `ripr first-pr` / `ripr start-here`. Also the flag source for
/// unknown-argument suggestions, so accepted flags have to appear as
/// option-list lines, not only inside the usage brackets.
pub(crate) const FIRST_PR_HELP: &str = "\
Summarize what to fix in one PR from existing ripr reports.

Answers \"what should I do first on this PR?\" first-pr reads reports that
earlier ripr commands wrote (check output, the gap list, review comments,
receipts) and writes one start-here page, start-here.md and start-here.json,
naming a single next step. A gap is changed behavior that no current test
appears to check. By default first-pr runs no analysis: for a missing report
it prints the command that creates it. Pass --run-chain to run those commands.

Usage: ripr first-pr|start-here [--root <path>] [--base <rev>] [--head <rev>] [--check-output <path>] [--gap-ledger <path>] [--first-action <path>] [--review-comments <path>] [--agent-packet <path>] [--gate-decision <path>] [--receipts-dir <path>] [--out-dir <path>] [--check] [--run-chain]

Options:
  --root <path>              Workspace root. Defaults to .
  --base <rev>               PR base revision. When omitted, resolved like
                             `ripr check`: origin/HEAD, then origin/main,
                             origin/master, main, and master.
  --head <rev>               PR head revision. Defaults to HEAD.
  --check-output <path>      Existing `ripr check --json` output to build the gap list from.
                             Without --run-chain, first-pr never runs analysis itself.
  --gap-ledger <path>        Gap list (gap-decision ledger) JSON. Defaults to target/ripr/reports/gap-decision-ledger.json.
  --first-action <path>      First-useful-action JSON. Defaults to target/ripr/reports/first-useful-action.json.
  --review-comments <path>   Review-comments JSON. Defaults to target/ripr/review/comments.json.
  --agent-packet <path>      Agent handoff JSON. Defaults to target/ripr/workflow/agent-packet.json.
  --gate-decision <path>     Gate-decision JSON. Defaults to target/ripr/reports/gate-decision.json.
  --receipts-dir <path>      Directory of repair receipts (before/after records). Defaults to target/ripr/receipts.
  --out-dir <path>           Output directory. Defaults to target/ripr/reports.
  --check                    Verify the existing start-here page is up to date; write nothing.
  --run-chain                Run the commands for missing reports first, each only when its
                             report is missing and each exactly the command first-pr prints,
                             with the resolved --base and --head: the repo-exposure report (a
                             `ripr check` run on a Python or TypeScript root without a Cargo
                             manifest), the gap list, then the review cards. This runs analysis
                             (cost scales with the repository), writes the missing reports (by
                             default under target/ripr/) and uses no network. The first failing
                             step stops with exit 2 and no start-here page is written from
                             partial reports. Cannot be combined with --check or --check-output.

Exit status:
  0  the start-here page was written, or --check found a current, unblocked page
  2  usage error, unresolvable input, or --check found a missing, stale or malformed page,
     or a current page that records a blocked state (wrong root, missing or malformed reports)
  Write mode exits 0 after composing a page, including a recovery page; read its
  State line (or the JSON `selected.output_state`), or run --check, to gate on
  the page state.

Start-here language:
  - start here: open target/ripr/reports/start-here.md first when it exists
  - safe next action: fix one named gap, regenerate a missing report, or stop because nothing needs doing
  - missing artifact / stale evidence / wrong root / malformed artifact: a report is absent, out of date,
    from another directory or unreadable; first-pr stops before suggesting any fix
  - no actionable gap: nothing to fix was found; advisory, not a promise that the tests catch every bug
  - preview-limited evidence: from a syntax-only preview analysis; advisory, with its limits stated before any fix
  - receipt lifecycle: receipt_missing, receipt_found, receipt_stale, receipt_gap_mismatch, receipt_movement_improved, receipt_movement_unchanged, receipt_not_applicable
  - verify command / receipt command / receipt path: the commands that recheck a fix and record its
    before/after result, and where that record is written
";
