<!-- section: Internal / CI -->
- `ripr doctor`'s directory-creation refusal is now tested with an injected `create_dir` failure, so the reason and both cache routes run on hosted Windows without relying on an ACL deny. The real-ACL tests fail with a named `NOT_ESTABLISHED` reason on a host that does not enforce the deny instead of passing silently (#9688).
