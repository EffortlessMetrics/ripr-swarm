<!-- section: Added -->
- Python related-test rows now carry the `no_call_path` miss when a test that
  only references a function owner (`assert callable(owner)`, `owner is None`,
  `owner.__name__`, a bare `assert owner`) is provably closed: every call is a
  vouched builtin or test-API call, and no fixture, hook, conftest
  declaration, module effect or dynamic construct can reach the owner
  (#10040, RIPR-SPEC-0224). The claim is withheld for a module whose
  `def`/`class` headers (decorators, defaults, annotations, bases),
  conditional imports or module-level statements use a call or an imported
  object, and for a body that imports or uses an imported object. Any other
  use or mention of the owner, a method or constant owner, and every
  TypeScript row keep `miss: null`. Accepted recall costs, all fail-closed:
  a module docstring naming the owner or an imported name, `X = sys.platform`,
  defs annotated with imported types, `skipif(sys.version_info < (3, 8))`, and
  module strings holding `(` or the word import.
- A TypeScript or JavaScript `missing_input` row is worded "no input this test
  passes was seen ..." instead of "no test input ...", so a single-file reading
  cannot over-claim by prose (#10040). No TypeScript producer emits it yet.
