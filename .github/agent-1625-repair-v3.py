from pathlib import Path

patch_path = Path(".github/agent-1625-patch.py")
patch = patch_path.read_text(encoding="utf-8")

for old, new in [
    (
        r'''r"fn as_str\(self\) -> &'static str \{\n\s*match self \{\n"''',
        r'''r"impl DocumentStalenessReason \{.*?fn as_str\(self\) -> &'static str \{\n\s*match self \{\n"''',
    ),
    (
        r'''r"fn description\(self\) -> &'static str \{\n\s*match self \{\n"''',
        r'''r"impl DocumentStalenessReason \{.*?fn description\(self\) -> &'static str \{\n\s*match self \{\n"''',
    ),
]:
    count = patch.count(old)
    if count != 1:
        raise SystemExit(f"expected one patch-pattern anchor, found {count}: {old}")
    patch = patch.replace(old, new, 1)

for old, new in [
    (r"b'\r'", r"b'\\r'"),
    (r"b'\n'", r"b'\\n'"),
    (r'"alpha\r\n😀x"', r'"alpha\\r\\n😀x"'),
    (r'"alpha\r\n😀y!"', r'"alpha\\r\\n😀y!"'),
]:
    if old not in patch:
        raise SystemExit(f"missing escape anchor: {old}")
    patch = patch.replace(old, new)

old_write = r'''    spec_path.write_text(spec_text, encoding="utf-8")'''
new_write = r'''    spec_path.write_text(spec_text.rstrip() + "\n", encoding="utf-8")'''
if patch.count(old_write) != 1:
    raise SystemExit("expected one spec write anchor")
patch = patch.replace(old_write, new_write, 1)

patch_path.write_text(patch, encoding="utf-8")
namespace = {"__name__": "__main__", "__file__": str(patch_path)}
exec(compile(patch, str(patch_path), "exec"), namespace)
