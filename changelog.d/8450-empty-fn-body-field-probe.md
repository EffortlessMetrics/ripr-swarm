<!-- section: Internal / CI -->
- Unit and `check_workspace` pins that a literally empty `fn` body elsewhere
  in the file still synthesizes the field-initializer probe (`total: a + b + 0`)
  and evaluates the element pin, instead of widening to the whole array line
  (#8450).
