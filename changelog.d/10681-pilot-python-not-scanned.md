<!-- section: Fixed -->
- `ripr pilot` no longer prints `findings: 0` / `no_python_findings` for a Python repo whose changed code `ripr check` reports findings for: when pilot's repo-wide pass has no Python findings, the terminal and markdown summaries now say `not scanned by pilot; run ripr check` instead of a numeric zero. The JSON `python_first_use` projection is unchanged (#10681).
