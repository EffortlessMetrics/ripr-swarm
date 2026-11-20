<!-- section: Fixed -->
- Python analysis reads a function or test body once when it decides which
  matches of a name sit in comments or strings, instead of once per match: a
  260 KB one-line comprehension chain that took 16 s to check now takes 0.4 s,
  with the same answers (#14195).
