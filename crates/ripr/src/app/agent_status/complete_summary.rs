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
use crate::output::markdown::powershell_text_variant;

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
    let repair = format!("ripr repair --root {root_arg}");
    rendered.push_str(&format!(
        "Next: `{repair}` starts the next gap, lists the gaps to choose from, or says why none is ready.\n"
    ));
    push_powershell_variant(&mut rendered, &repair);
    let status = format!(
        "ripr status --attempt {} --root {root_arg}",
        shell_arg(&attempt.attempt_id)
    );
    rendered.push_str(&format!("Full status: `{status}`\n"));
    push_powershell_variant(&mut rendered, &status);
    rendered
}

/// A PowerShell line under a printed command only when its spelling differs
/// from Bash (for example a root with an apostrophe), as the status page's
/// next action does.
fn push_powershell_variant(rendered: &mut String, command: &str) {
    if let Some(variant) = powershell_text_variant(command) {
        rendered.push_str(&format!("  (PowerShell) `{variant}`\n"));
    }
}

#[cfg(test)]
mod tests {
    use super::push_powershell_variant;

    #[test]
    fn a_plain_command_gets_no_powershell_line() {
        let mut rendered = String::new();
        push_powershell_variant(&mut rendered, "ripr repair --root /work/proj");
        assert_eq!(rendered, "");
    }

    #[test]
    fn an_apostrophe_root_gets_the_powershell_spelling() {
        let mut rendered = String::new();
        push_powershell_variant(&mut rendered, "ripr repair --root '/home/o'\\''neil/proj'");
        assert_eq!(
            rendered,
            "  (PowerShell) `ripr repair --root '/home/o''neil/proj'`\n"
        );
    }
}
