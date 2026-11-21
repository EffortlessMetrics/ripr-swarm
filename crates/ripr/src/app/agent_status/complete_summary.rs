//! The short human summary `ripr continue` prints for an attempt that is
//! already complete (#15282 item 7).
//!
//! The full `# RIPR Repair Attempt Status` page stays with `ripr status`;
//! a repeated `continue` only needs what finished, the receipt it left,
//! whether a test run was recorded, and where to go next. Receipt and
//! test-run wording come from the status page's own helpers, so the two
//! surfaces cannot disagree about the same attempt.

use super::{AgentAttemptStatusReport, receipt_summary, test_run};
use crate::agent::loop_commands::shell_arg;

/// Render the already-complete summary. `root_arg` is the shell-quoted
/// `--root` value the printed commands use.
pub(crate) fn render_already_complete_summary(
    report: &AgentAttemptStatusReport,
    root_arg: &str,
) -> String {
    let attempt = &report.attempt;
    let mut rendered = format!(
        "Repair attempt `{}` is complete ({}).\n",
        attempt.attempt_id, attempt.status_class
    );
    if let Some(seam_id) = &attempt.seam_id {
        rendered.push_str(&format!("Gap: `{seam_id}`\n"));
    }
    if let Some(receipt) = &attempt.receipt {
        rendered.push_str(&format!("Receipt: {}\n", receipt_summary(receipt)));
    }
    if let Some(reading) = &report.test_run {
        rendered.push_str(&format!("{}\n", test_run::test_run_line(reading)));
    }
    rendered.push_str(&format!(
        "Next: `ripr repair --root {root_arg}` starts the next gap, or says why none is ready.\n"
    ));
    rendered.push_str(&format!(
        "Full status: `ripr status --attempt {} --root {root_arg}`\n",
        shell_arg(&attempt.attempt_id)
    ));
    rendered
}
