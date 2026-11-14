//! Tests for an import line edited only to import more names (#14157).

use super::import_adds_binding::import_line_only_adds_bindings;
use super::type_checking_block_tests::{analyze, line, probed_lines};

/// Line 3 and line 4 are the import lines; line 8 is runtime code that uses
/// `cast`.
const PRICING_PY: &str = "from __future__ import annotations\n\nimport os, sys\nfrom typing import Any, cast\n\n\ndef total(prices: Any, basket: Any) -> int:\n    return sum(cast(int, prices[item]) for item in basket)\n";

const OLD_RETURN: &str = "    return sum(prices[item] for item in basket)";

/// The requests shape: `cast` joins an existing `from typing import` line in
/// the commit that starts using it. The import line is no probe; the runtime
/// line carries the change. Without the guard line 4 reads as a `predicate`
/// probe on `<module>`.
#[test]
fn name_added_to_a_from_import_is_not_probed() -> Result<(), String> {
    let findings = analyze(
        "from-adds-name",
        PRICING_PY,
        &[4, 8],
        vec![line(4, "from typing import Any"), line(8, OLD_RETURN)],
    )?;
    assert_eq!(probed_lines(&findings), vec![8], "{findings:?}");
    Ok(())
}

#[test]
fn module_added_to_a_plain_import_is_not_probed() -> Result<(), String> {
    let findings = analyze(
        "import-adds-module",
        PRICING_PY,
        &[3],
        vec![line(3, "import os")],
    )?;
    assert_eq!(probed_lines(&findings), Vec::<usize>::new(), "{findings:?}");
    Ok(())
}

/// Controls on the same file and the same added line: each edit changes what
/// an existing name refers to, so the import line keeps its probe.
#[test]
fn import_edits_that_keep_their_probe() -> Result<(), String> {
    for (label, old_line) in [
        ("module-changed", "from types import Any"),
        ("binding-dropped", "from typing import Any, Optional"),
        ("alias-dropped", "from typing import Any as Anything"),
        ("level-changed", "from .typing import Any"),
    ] {
        let findings = analyze(
            label,
            PRICING_PY,
            &[4, 8],
            vec![line(4, old_line), line(8, OLD_RETURN)],
        )?;
        assert_eq!(probed_lines(&findings), vec![4, 8], "{label}: {findings:?}");
    }
    Ok(())
}

/// Line 8 is unchanged here, so the old file already used `cast`: the added
/// import rebinds a name that meant something before.
#[test]
fn added_name_the_old_file_already_used_keeps_probe() -> Result<(), String> {
    let findings = analyze(
        "name-already-used",
        PRICING_PY,
        &[4],
        vec![line(4, "from typing import Any")],
    )?;
    assert_eq!(probed_lines(&findings), vec![4], "{findings:?}");
    Ok(())
}

#[test]
fn import_line_shapes() {
    let old_source = "import os\nvalue = shadowed\n";
    for (old, new, expected) in [
        (
            "from typing import Any",
            "from typing import Any, cast",
            true,
        ),
        (
            "from typing import Any",
            "from typing import cast, Any",
            true,
        ),
        (
            "from typing import Any",
            "from typing import (Any, cast)",
            true,
        ),
        (
            "from typing import Any",
            "from typing import (Any, cast,)",
            true,
        ),
        (
            "from typing import Any  # noqa",
            "from typing import Any, cast",
            true,
        ),
        ("from . import a", "from . import a, b", true),
        (
            "from ..pkg.mod import a as x",
            "from ..pkg.mod import a as x, b as y",
            true,
        ),
        (
            "    from typing import Any",
            "    from typing import Any, cast",
            true,
        ),
        ("import os", "import os, sys", true),
        ("import os.path as p", "import os.path as p, json", true),
        // Not only an addition.
        ("from typing import Any", "from typing import Any", false),
        (
            "from typing import Any, cast",
            "from typing import Any",
            false,
        ),
        ("from typing import Any", "from typing import cast", false),
        (
            "from typing import Any",
            "from typing import Any as A, cast",
            false,
        ),
        (
            "from typing import Any",
            "from typing_extensions import Any, cast",
            false,
        ),
        (
            "from .typing import Any",
            "from typing import Any, cast",
            false,
        ),
        (
            "from typing import Any",
            "    from typing import Any, cast",
            false,
        ),
        ("import os", "from os import path, sep", false),
        // The load order of a plain import list changed.
        ("import os, json", "import json, os, sys", false),
        // The added name was already in the old file.
        (
            "from typing import Any",
            "from typing import Any, shadowed",
            false,
        ),
        (
            "from typing import Any",
            "from typing import Any, cast as shadowed",
            false,
        ),
        ("import json", "import json, os.path", false),
        // Shapes that are not read.
        ("from typing import Any", "from typing import *", false),
        ("from typing import *", "from typing import *, cast", false),
        (
            "from typing import Any",
            "from typing import (Any, cast",
            false,
        ),
        (
            "from typing import Any",
            "from typing import Any, cast; cast = 1",
            false,
        ),
        (
            "from typing import Any",
            "from typing import Any, \\",
            false,
        ),
        (
            "from __future__ import division",
            "from __future__ import division, annotations",
            false,
        ),
        ("import os", "import (os, sys)", false),
        ("value = 1", "from typing import Any, cast", false),
    ] {
        assert_eq!(
            import_line_only_adds_bindings(old, new, old_source),
            expected,
            "{old:?} -> {new:?}"
        );
    }
}
