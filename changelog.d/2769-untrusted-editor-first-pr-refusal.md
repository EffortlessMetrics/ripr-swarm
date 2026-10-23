<!-- section: Internal / CI -->
- The untrusted editor journey now asserts that diagnostic-scoped first-pr copies (repair packet, verify and receipt commands) stay refused in an untrusted workspace instead of waiting for a clipboard copy that the trust boundary correctly prevents; the two smoke tests that expected the copy failed the Windows untrusted journey (#2769).
