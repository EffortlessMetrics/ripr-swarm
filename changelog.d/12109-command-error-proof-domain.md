<!-- section: Internal / CI -->
- Guard the command-error Kani input domain against omitted, duplicated, or newly
  added constructors, and witness each input independently of the exit-code
  result while retaining the literal 2/3/130 contract oracle (#12109).
