# RIPR-SPEC-0438: Long Flag `--name=value` Spelling

Status: accepted

Owner: product / cli

Created: 2026-11-11

Linked issues:

- [#14292](https://github.com/EffortlessMetrics/ripr-swarm/issues/14292) -
  every command accepts `--name=value` as well as `--name value`.

## Problem

Every ripr command parser matched whole tokens, so `ripr check
--format=json`, `--base=origin/main` and `ripr context --at=ID` were refused
as unknown arguments, even though most CLI tools accept that spelling and
agents and scripts emit it.

## Behavior

One owner, `cli::parse::split_long_flag_values`, rewrites argv once, before
CLI dispatch (`cli::run`) and before the binary's MCP route
(`startup::routed_mcp_args`):

1. A token of the form `--name=value` (name of ASCII alphanumerics and `-`)
   becomes `--name` followed by `value`, split at the first `=`. `--root=`
   becomes `--root` and an empty value, which the owning parser refuses as
   it would `--root ""`.
2. A documented switch is never split: a flag that every command help
   documents without a value (`--quiet`, `--json`), and the global
   `--verbose`, `--help` and `--version`. `--quiet=false` reaches the
   command parser whole and is refused as typed. Whether a flag takes a value
   comes from the same help inventory as `ripr help --json`. A lowercase
   choice list (`--phase before|after|verify`) documents a value.
3. A token in the value position of a bare documented value-taking flag
   stays whole: `--base --output=x` keeps `--output=x` as the base. A value
   that came from splitting never owns the next token.
4. `argv[0]`, the dash-led tokens before the command word (so
   `ripr --format=json --version` stays a version request), short flags,
   positionals and every token after a literal `--` are unchanged.

`ripr help --all` documents the rule.

## Required Evidence

- The `=` spelling produces the same parsed command as the spaced spelling.
- Switches, global switches, value-position tokens, leading top-level flags
  and tokens after `--` stay unchanged.
- `--root=a` names a missing `--root` directory, which shows the split
  reached the owner.

## Non-Goals

- Short-flag `-x=value` or `-xvalue` spellings.
- Accepting a value on a switch (`--quiet=false` stays refused).

## Acceptance Examples

1. `ripr check --format=json --base=main` behaves like
   `ripr check --format json --base main`.
2. `ripr check --quiet=false` exits 2 with
   `unknown check argument "--quiet=false"`.
3. `ripr agent repair --attempt ID --phase=after` reads `--phase after`.

## Test Mapping

- `crates/ripr/src/cli/parse.rs::tests::split_long_flag_values_rewrites_only_long_flag_assignments`
- `crates/ripr/src/cli/parse.rs::tests::split_long_flag_values_keeps_a_value_position_token_whole`
- `crates/ripr/src/cli/parse.rs::tests::split_long_flag_values_splits_a_choice_list_flag_and_not_a_split_off_value`
- `crates/ripr/src/cli/parse.rs::tests::split_long_flag_values_keeps_global_switch_assignments_whole`
- `crates/ripr/src/cli/parse.rs::tests::split_long_flag_values_leaves_flags_before_the_command_alone`
- `crates/ripr/src/cli/parse.rs::tests::split_long_flag_values_leaves_argv0_and_tokens_after_double_dash`
- `crates/ripr/src/cli/parse.rs::tests::split_long_flag_values_feeds_the_same_command_as_the_spaced_spelling`
- `crates/ripr/src/cli/help_flags.rs::tests::a_lowercase_choice_list_takes_a_value`
- `crates/ripr/src/cli/mod.rs::tests::run_reads_flag_equals_value_like_the_spaced_spelling`
- `crates/ripr/src/startup.rs::tests::startup_splits_flag_equals_value_on_the_mcp_route`
- `crates/ripr/tests/cache_clear_safety.rs`

## Implementation Mapping

- `crates/ripr/src/cli/parse.rs`: `split_long_flag_values`, `documented_arity`.
- `crates/ripr/src/cli/help_flags.rs`: `is_metavar` choice lists.
- `crates/ripr/src/cli/mod.rs`: the rewrite in `run_with_policy`.
- `crates/ripr/src/startup.rs`: the rewrite on the MCP route.
- `crates/ripr/src/cli/help/overview.rs`: the `help --all` disclosure.

## Metrics

- `unit_test_pass_rate`: the tests above are required Rust tests.
