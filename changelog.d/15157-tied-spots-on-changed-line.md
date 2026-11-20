<!-- section: Fixed -->
- `ripr check`: on a changed line that holds several comparisons, such as `a < b && c == d`, the repair-start note now names the spots tied on that line and says ripr will not pick one. It used to say that no spot could be tied to the changed line, quote the count of every spot in the file, and point at a `ripr pilot` summary that ranked nothing (#15157).
