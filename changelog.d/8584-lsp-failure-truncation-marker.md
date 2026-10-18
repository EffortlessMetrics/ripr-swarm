<!-- section: Fixed -->
- LSP `ripr/analysisStatus` failure summaries still redact paths and stay
  within 240 characters, and a truncated `failure.message` now ends with
  `…` inside that bound so a client can tell that text was dropped (#8584).
