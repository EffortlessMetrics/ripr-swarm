<!-- section: Fixed -->
- Python preview: a changed line in the body of `if TYPE_CHECKING:` (or
  `if typing.TYPE_CHECKING:`) no longer produces a finding. That body never
  runs, so an import or alias edited, added or deleted there changes no
  behavior. Runtime code moved into such a block, the `else` branch, and a
  module that rebinds `TYPE_CHECKING` keep their findings (#14156).
