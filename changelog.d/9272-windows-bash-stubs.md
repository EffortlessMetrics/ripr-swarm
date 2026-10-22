<!-- section: Fixed -->
- Windows: the admission wrapper and its selftests now run on Windows. The packet decode strips `\r` (jq.exe emits CRLF), the admission helper resolves `gh` through PATHEXT, and the selftests write scripts to files and build PATH with the host separator (#9272).
