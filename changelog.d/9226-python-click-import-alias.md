<!-- section: Fixed -->
- Python preview: `import click as <alias>` now qualifies `@<alias>.command` / `@<alias>.group` owners and same-file group subcommands for the weak reach-only `cli_runner_invoke` relation when the alias is the module's sole stdlib `click` binding and is not rebound; never `exposed` (#9226).
