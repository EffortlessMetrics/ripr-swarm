<!-- section: Fixed -->
- The generated workflow's pull request summary comment no longer says RIPR "does not block merging" when `RIPR_GATE_MODE` lets the gate fail the job. It names the gate mode in its footer, and states a blocked gate, a gate configuration error or an unknown mode at the top, matching the failing annotation and the job summary (#15718).
