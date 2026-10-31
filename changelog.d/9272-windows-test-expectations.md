<!-- section: Fixed -->
- Windows test expectations follow the host quoting and root spelling `ripr` prints (double-quoted arguments, forward-slash roots and custody paths), the nested `cargo build -p ripr` that Windows refuses while a test runs `ripr.exe` is retried for about a minute, and a retained-gap binding refusal now names which of its three halves failed (#9272).

Retained-gap binding compares canonical forms of both the recorded and the live root, so a Windows verbatim (`\\?\`) path from `canonicalize` no longer refuses an unmoved root.
