<!-- section: Added -->
- Python related-test rows now carry the `no_call_path` miss when a test that
  only references a function owner (`assert callable(owner)`, `owner is None`,
  `owner.__name__`, a bare `assert owner`) is provably closed: every call is a
  vouched builtin or test-API call, and no fixture, hook, conftest
  declaration, module effect or dynamic construct can reach the owner
  (#10040, RIPR-SPEC-0224). Any other use or mention of the owner, a method or
  constant owner, and every TypeScript row keep `miss: null`.
- A TypeScript or JavaScript `missing_input` row is worded "no input this test
  passes was seen ..." instead of "no test input ...", so a single-file reading
  cannot over-claim by prose (#10040). No TypeScript producer emits it yet.
