<!-- section: Fixed -->

- The Windows command-shell disclosure now says a command that begins with a `NAME=value` environment assignment (for example the seam-limited pilot snapshot commands) is Bash syntax only, not valid in cmd.exe or PowerShell as written, and names the PowerShell form or a separate `set NAME=value` line as the route (#12217).
