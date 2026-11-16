<!-- section: Changed -->
- The default `ripr check` `Hidden:` list states a long shared reason once:
  a row whose reason repeats the row above prints `reason: same as above`.
  Across the 20 affected fixtures this removes 7.8 KB of repeated limit text
  (#14308). `--format human-full` and machine formats are unchanged.
