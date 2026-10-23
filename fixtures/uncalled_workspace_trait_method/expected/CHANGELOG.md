# Golden Output Changes

## Pending — uncalled_workspace_trait_method (1)

Reason:
RIPR-SPEC-0001: a method of a workspace-declared trait no test calls is no_static_path, not a borrowed oracle (#9169)

Command:
`cargo xtask goldens bless uncalled_workspace_trait_method --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
