# Golden Output Changes

## Pending — python_module_string_reader_short_circuit (1)

Reason:
RIPR-SPEC-0028 and RIPR-SPEC-0108: retain the confirmed false-`and` reader
counterexample as a nonempty, non-promoting finding. The baseline credited
the old-value conjunct even though the reader is never called.

Command:
`ripr check --root fixtures/python_module_string_reader_short_circuit/input --diff fixtures/python_module_string_reader_short_circuit/diff.patch --mode fast --json`

The same command used `--legend` for `human.txt` and `--format human-full`
for `human-full.txt`. Capture used the verified native artifact for
`bb525546ad7af224503d57a4a6de717cb5b877aa`, `RAYON_NUM_THREADS=1` and one fresh
shared fixture cache. Only the repository fixture normalizations were
applied: remove the optional canonical-next-action/language-completeness
members, normalize the root closer, and replace the checkout prefix with
`<cwd>/`. Source text and input digests were preserved. No local
`cargo xtask goldens bless` execution is claimed.

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`
