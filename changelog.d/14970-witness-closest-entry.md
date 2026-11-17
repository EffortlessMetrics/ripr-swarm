<!-- section: Fixed -->
- `ripr explain` and `ripr check`: the "For example, the test ..." pointer on
  the transitive-reach limitation now names the test that enters closest to
  the change, instead of the first test by file name. A test that calls a
  same-named method (`Options::new(5).break_words(false)`) no longer outranks
  a real call to the free function. On textwrap, an edit in
  `Word::break_apart` now points at a `wrap` test rather than
  `wrap_columns_empty_text` five hops away (#14970).
