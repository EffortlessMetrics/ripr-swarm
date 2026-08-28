from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
INIT = ROOT / "crates/ripr/src/cli/commands/init.rs"
BADGE_ADOPTION = ROOT / "docs/BADGE_ADOPTION.md"


def replace_once(path: Path, old: str, new: str) -> None:
    text = path.read_text(encoding="utf-8")
    count = text.count(old)
    if count != 1:
        raise RuntimeError(
            f"expected exactly one match in {path.relative_to(ROOT)}; found {count}: {old!r}"
        )
    path.write_text(text.replace(old, new, 1), encoding="utf-8")


replace_once(
    INIT,
    '''        write(
            &workflow,
            "name: existing
",
        )?;
''',
    '        write(&workflow, "name: existing\\n")?;\n',
)

replace_once(
    BADGE_ADOPTION,
    "README badges and the narrower, preconditioned `ripr+` badge.\n\n\n## Generated GitHub workflows",
    "README badges and the narrower, preconditioned `ripr+` badge.\n\n## Generated GitHub workflows",
)

replace_once(
    BADGE_ADOPTION,
    "3. Keep the generated badge workflow aligned with released RIPR versions and reviewed action pins.\n",
    "3. Keep the generated badge workflow aligned with released RIPR\n   versions and reviewed action pins.\n",
)

print("Applied final issue #2017 review cleanup.")
