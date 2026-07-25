from pathlib import Path
import re


def replace_count(path: str, old: str, new: str, expected: int, label: str) -> None:
    target = Path(path)
    text = target.read_text(encoding="utf-8")
    count = text.count(old)
    if count != expected:
        raise SystemExit(f"{label}: expected {expected} matches in {path}, found {count}")
    target.write_text(text.replace(old, new), encoding="utf-8")


replace_count(
    "crates/ripr/src/app/agent_status.rs",
    '--root \\"repo root\\"',
    "--root 'repo root'",
    2,
    "agent-status path expectations",
)
replace_count(
    "crates/ripr/src/lsp/tests.rs",
    '--base \\"origin/main with space\\"',
    "--base 'origin/main with space'",
    1,
    "LSP command payload expectation",
)
replace_count(
    "crates/ripr/src/output/receipt_write.rs",
    '--verify-command \\"cargo xtask fixtures boundary_gap\\"',
    "--verify-command 'cargo xtask fixtures boundary_gap'",
    2,
    "receipt-write expectations",
)
replace_count(
    "crates/ripr/src/output/first_pr.rs",
    'git -C . rev-parse --verify \\"missing-head^{commit}\\"',
    "git -C . rev-parse --verify 'missing-head^{commit}'",
    1,
    "missing-head recovery expectation",
)
replace_count(
    "crates/ripr/src/output/first_pr.rs",
    '--gap-id \\"gap:pr:gap:python:app/pricing.py:calculate_discount:predicate_boundary:amount>=threshold\\"',
    "--gap-id 'gap:pr:gap:python:app/pricing.py:calculate_discount:predicate_boundary:amount>=threshold'",
    1,
    "Python agent-packet expectation",
)
replace_count(
    "fixtures/boundary_gap/expected/llm-work-loop/path-with-spaces/review-summary.json",
    '--root \\\"repo root\\\"',
    "--root 'repo root'",
    2,
    "review-summary fixture path arguments",
)
replace_count(
    "fixtures/first_successful_pr/boundary-gap/expected/start-here.json",
    '--verify-command \\\"cargo xtask fixtures boundary_gap\\\"',
    "--verify-command 'cargo xtask fixtures boundary_gap'",
    2,
    "boundary-gap JSON receipt commands",
)
replace_count(
    "fixtures/first_successful_pr/boundary-gap/expected/start-here.md",
    '--verify-command \"cargo xtask fixtures boundary_gap\"',
    "--verify-command 'cargo xtask fixtures boundary_gap'",
    2,
    "boundary-gap Markdown receipt commands",
)
replace_count(
    "fixtures/first_successful_pr/python-preview-gap/expected/start-here.json",
    '--gap-id \\\"gap:pr:gap:python:app/pricing.py:calculate_discount:predicate_boundary:amount>=threshold\\\"',
    "--gap-id 'gap:pr:gap:python:app/pricing.py:calculate_discount:predicate_boundary:amount>=threshold'",
    3,
    "Python JSON agent-packet commands",
)
replace_count(
    "fixtures/first_successful_pr/python-preview-gap/expected/start-here.md",
    '--gap-id \"gap:pr:gap:python:app/pricing.py:calculate_discount:predicate_boundary:amount>=threshold\"',
    "--gap-id 'gap:pr:gap:python:app/pricing.py:calculate_discount:predicate_boundary:amount>=threshold'",
    1,
    "Python Markdown agent-packet command",
)

# Every first-PR fixture renders receipt commands through the same `shell_arg`
# authority. Migrate the remaining corpus rather than waiting for the fixture
# loop to stop at one directory per run.
fixture_root = Path("fixtures/first_successful_pr")
json_changes = 0
markdown_changes = 0
for path in fixture_root.glob("*/expected/start-here.json"):
    text = path.read_text(encoding="utf-8")
    text, count = re.subn(
        r'--verify-command \\\"([^\"]+)\\\"',
        lambda match: f"--verify-command '{match.group(1)}'",
        text,
    )
    if count:
        json_changes += count
        path.write_text(text, encoding="utf-8")
for path in fixture_root.glob("*/expected/start-here.md"):
    text = path.read_text(encoding="utf-8")
    text, count = re.subn(
        r'--verify-command \"([^\"]+)\"',
        lambda match: f"--verify-command '{match.group(1)}'",
        text,
    )
    if count:
        markdown_changes += count
        path.write_text(text, encoding="utf-8")
if json_changes == 0 or markdown_changes == 0:
    raise SystemExit(
        "first-PR receipt corpus migration found no remaining JSON or Markdown double-quoted commands"
    )
