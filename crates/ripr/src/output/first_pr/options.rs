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
Create the start-here packet for one PR from existing RIPR artifacts.
By default first-pr runs no analysis: it prints the next command for a missing
artifact. Pass --run-chain to have it run the missing-artifact chain itself.

Usage: ripr first-pr|start-here [--root <path>] [--base <rev>] [--head <rev>] [--check-output <path>] [--gap-ledger <path>] [--first-action <path>] [--review-comments <path>] [--agent-packet <path>] [--gate-decision <path>] [--receipts-dir <path>] [--out-dir <path>] [--check] [--run-chain]

Options:
  --root <path>              Workspace root. Defaults to .
  --base <rev>               PR base revision. When omitted, resolved like
                             `ripr check`: origin/HEAD, then origin/main,
                             origin/master, main, and master.
  --head <rev>               PR head revision. Defaults to HEAD.
  --check-output <path>      Existing `ripr check --json` output to derive the gap ledger from.
                             Without --run-chain, first-pr never runs analysis itself.
  --gap-ledger <path>        Gap-decision ledger JSON. Defaults to target/ripr/reports/gap-decision-ledger.json.
  --first-action <path>      First-useful-action JSON. Defaults to target/ripr/reports/first-useful-action.json.
  --review-comments <path>   Review-comments JSON. Defaults to target/ripr/review/comments.json.
  --agent-packet <path>      Agent packet JSON. Defaults to target/ripr/workflow/agent-packet.json.
  --gate-decision <path>     Gate-decision JSON. Defaults to target/ripr/reports/gate-decision.json.
  --receipts-dir <path>      Receipt directory. Defaults to target/ripr/receipts.
  --out-dir <path>           Output directory. Defaults to target/ripr/reports.
  --check                    Verify the existing start-here packet is up to date.
  --run-chain                Opt in to running the missing-artifact chain first: the repo-exposure
                             report (a `ripr check` run on a Python or TypeScript root without a
                             Cargo manifest), the gap ledger, then the review cards, each only when missing
                             and each exactly the command first-pr prints, with the resolved
                             --base and --head. This runs analysis (cost scales with the repository),
                             writes the missing artifacts (by default under target/ripr/) and
                             uses no network. The first failing step stops the chain with exit 2 and
                             no start-here packet is written from partial artifacts. Cannot be
                             combined with --check or --check-output.

Exit status:
  0  the start-here packet was written, or --check found a current, unblocked packet
  2  usage error, unresolvable input, or --check found a missing, stale or malformed packet,
     or a current packet that records a blocked state (wrong root, missing or malformed artifacts)
  Write mode exits 0 after composing a packet, including a recovery packet; read its
  State line (or the JSON `selected.output_state`), or run --check, to gate on
  the packet state.

Start-here language:
  - start here: open target/ripr/reports/start-here.md first when it exists
  - safe next action: repair one named gap, regenerate missing evidence, or stop on no-action
  - missing artifact / stale evidence / wrong root / malformed artifact: fail closed before repair work
  - no actionable gap: advisory no-action, not runtime adequacy or mutation proof
  - preview-limited evidence: syntax-first and advisory, with static limits before repair language
  - receipt lifecycle: receipt_missing, receipt_found, receipt_stale, receipt_gap_mismatch, receipt_movement_improved, receipt_movement_unchanged, receipt_not_applicable
  - verify command / receipt command / receipt path: static movement proof rail
";
