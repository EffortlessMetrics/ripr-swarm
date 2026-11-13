<!-- section: Fixed -->
- Python preview: an import line edited only to import more names
  (`from typing import Any` to `from typing import Any, cast`, or
  `import os` to `import os, sys`) no longer produces a finding on
  `<module>`. It is treated like a new import line. A changed module, a
  dropped binding, a changed alias, a star or `__future__` import, and an
  added name the old file already used keep their findings (#14157).
