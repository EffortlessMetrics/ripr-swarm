from pathlib import Path

patch_path = Path(".github/agent-1625-patch.py")
patch = patch_path.read_text(encoding="utf-8")
replacements = [
    (
        r'''r"fn as_str\(self\) -> &'static str \{\n\s*match self \{\n"''',
        r'''r"impl DocumentStalenessReason \{.*?fn as_str\(self\) -> &'static str \{\n\s*match self \{\n"''',
    ),
    (
        r'''r"fn description\(self\) -> &'static str \{\n\s*match self \{\n"''',
        r'''r"impl DocumentStalenessReason \{.*?fn description\(self\) -> &'static str \{\n\s*match self \{\n"''',
    ),
]
for old, new in replacements:
    count = patch.count(old)
    if count != 1:
        raise SystemExit(f"expected one patch-pattern anchor, found {count}: {old}")
    patch = patch.replace(old, new, 1)

patch_path.write_text(patch, encoding="utf-8")
namespace = {"__name__": "__main__", "__file__": str(patch_path)}
exec(compile(patch, str(patch_path), "exec"), namespace)
