# Using ripr on Windows

ripr runs natively on Windows (`x86_64-pc-windows-msvc`). This page collects the
Windows-specific behavior that is otherwise spread across command output and
code comments.

## Install

The release archive is `ripr-server-v{version}-x86_64-pc-windows-msvc.zip`. The
PowerShell snippet in the [quickstart](QUICKSTART.md) downloads it, checks the
SHA-256 and puts `ripr.exe` on `PATH` for the current session. To keep it,
add the extracted directory to your user `Path` (search the Start menu for "Edit environment variables for your account"). See
[install channels](INSTALL_CHANNELS.md) for which channels exist today; the
npm and PyPI packages are Linux-only until more platforms publish.

## Which shell

Generated commands (workflow packets, repair loops, receipts) are quoted for the
shell family of the host that printed them. On Windows:

- **Git Bash and PowerShell are supported.** Commands use double quotes and `>`,
  which both honor for paths with spaces. A value containing `$` or a backtick
  keeps single quotes, so paste that one in Git Bash or PowerShell, not cmd.exe.
  A value containing `%` is also not safe in cmd.exe.
- **PowerShell forms are printed where a faithful translation exists.** They
  use the `&` call operator and a BOM-free UTF-8 write in place of `>`. When no
  honest translation exists (compound commands), the output says
  "PowerShell form unavailable" instead of guessing.
- **WSL is not a drop-in substitute.** Printed paths keep their drive-letter
  prefix, which WSL reads as a relative path. Rewrite them under `/mnt/` and
  install ripr inside WSL first.

## Paths

- Paths in ripr's findings and reports are written with `/` separators on every
  platform.
- Git is the authority for what changed. Diffs from Git for Windows work as
  they do elsewhere.

## Line endings

When Git would itself normalize a file's line endings (`core.autocrlf` or the
`text` attribute), a file whose only change between the before and after phases
of a repair attempt is a CRLF/LF flip is not reported as changed. Where Git would
not normalize it, the flip counts as an edit.

## Long paths

Windows refuses a process working directory longer than 258 UTF-16 units
(`MAX_PATH`), even with `LongPathsEnabled=1`. If the workspace root is over that
limit, ripr stops with "clone or move the repository to a shorter path". Move the
repository somewhere shallower, for example `C:\src\repo`.

## Editor

The VS Code extension downloads and manages its own server. The
[Windows packaged qualification](WINDOWS_PACKAGED_QUALIFICATION.md) lane is a
release rehearsal for a named candidate; it is not a per-install check.

## Reporting a problem

File an issue with `ripr --version`, your shell (Git Bash, PowerShell 5.1 or
7), whether the path has spaces, and the full command output.
