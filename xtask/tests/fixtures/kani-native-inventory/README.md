# Pinned native Kani inventory

These are retained native Kani 0.68.0 bytes, not authored solver output.
`receipt.json` binds source `55b89de2010b932881fa5b7171587c455dc6e84a`,
command, fresh working directory, before/after source observations, tool
configuration, raw output hashes and the native exit. The actual command was:

```text
cargo kani -p ripr-contract --manifest-path <checkout>/Cargo.toml list --format json
```

It completed with exit 0 in 25.95 seconds and listed 21 ordinary harnesses,
zero contract harnesses and zero functions under contract. The JSON is 2,533
bytes with SHA-256
`d023ab5a23a66e93b649312ef2f229af1659d04abd522330c26d587dc5fb7224`.
The command qualifies this native inventory format and argument ordering. It
executes no solver and establishes no harness property or claim adequacy.

An earlier argument order put variadic `-p` immediately before `list`; Kani
consumed `list` as another package and rejected `--format` (exit 2). A separate
`ripr` listing exhausted a 120-second bound without JSON; neither is a native
inventory pass. The retained successful control covers `ripr-contract` only.

The parser control reuses these exact bytes. Mutated version, duplicate key,
duplicate subject, missing/extra subject and inconsistent-total controls are
separate authored inputs. A source path or count alone is never source/run
provenance. The production custody producer still requires its own exact-head
process and native integration qualification.
