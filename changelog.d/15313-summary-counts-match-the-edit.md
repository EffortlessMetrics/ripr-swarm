<!-- section: Fixed -->
- `ripr check` Summary names the candidate-line count when several checks share
  one edited line ("2 changed behaviors checked on 1 candidate line"), and says
  "No test gap: N listed" instead of "N found" when `State:` reads "no test gap
  found", so the Summary matches the edit and never contradicts State (#15313).
