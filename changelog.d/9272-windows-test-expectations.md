<!-- section: Fixed -->
- Windows test expectations follow the host quoting and root spelling `ripr` prints (double-quoted arguments, forward-slash roots and custody paths), the nested `cargo build -p ripr` that Windows refuses while a test runs `ripr.exe` is retried for about a minute, and a retained-gap binding refusal now names which of its three halves failed (#9272).
- Retained-gap binding compares canonical forms of both the recorded and the live root, so a Windows verbatim-prefix path from `canonicalize` no longer refuses an unmoved root (#9272).
- First-PR root comparison treats a Windows verbatim-prefix (`\\?\`) ledger root as the same directory as its plain form, so a canonically recorded ledger is no longer reported as `wrong_root` (#9272).
