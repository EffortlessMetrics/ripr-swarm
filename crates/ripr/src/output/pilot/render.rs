mod complete;
mod render_helpers;
mod timeout;

pub(crate) use complete::{
    render_pilot_summary_json, render_pilot_summary_md, render_pilot_terminal,
};
pub(crate) use timeout::{
    render_pilot_timeout_summary_json, render_pilot_timeout_summary_md,
    render_pilot_timeout_terminal,
};

/// Shell disclosure shown before generated command blocks in pilot markdown.
///
/// The command strings are bash source (`agent::loop_commands::shell_arg`), so
/// each fenced `bash` block is paired with a PowerShell translation derived by
/// `powershell_command`. Naming both shells — and the cmd.exe boundary — in the
/// prose keeps the packet honest on Windows (#2628); the wording mirrors the
/// landed `agent_workflow` disclosure so every generated-command surface states
/// the same contract.
pub(super) const COMMAND_SHELL_DISCLOSURE: &str = "Each command includes Bash and PowerShell variants. The Bash form uses POSIX single-quote quoting and `>` redirection; the PowerShell form uses PowerShell's doubled-quote equivalent and UTF-8 `Out-File` redirection. cmd.exe is not supported. On Windows, use either Git Bash or PowerShell. WSL bash is not a drop-in substitute: paths keep their Windows drive-letter prefix, which WSL resolves as a relative path.\n\n";

pub(super) fn why_line(entry: &crate::analysis::ClassifiedSeam) -> String {
    render_helpers::why_line(entry)
}
