<!-- section: Changed -->
- The first-run walk now records each case's `verdict_reading` and the dx-scoreboard counts the crates whose verdict is decisive and agrees with what the tests catch as `first_run.decisive_verdicts` (higher is better, target 2 of 3, any drop regresses). An `*_unknown` verdict never counts (#9276).
