# Focused run stop record

An earlier post-commit focused runner recorded four exit-0 commands, then was terminated while goldens was in progress under the user's stop instruction. That partial run is retained in `focused-receipts.json` and `focused-logs/`; it is superseded by the completed exact-candidate run in `exact-a4/`. The completed run has exit 0 for the 13-case sibling suite, goldens, honesty, and dogfood, each with raw stdout, UTC finish time, commit/tree binding, and hashes for all four changed source files.
