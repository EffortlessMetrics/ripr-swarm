from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
CORE_HELP = ROOT / "crates/ripr/src/cli/help/core.rs"
HELP_TESTS = ROOT / "crates/ripr/src/cli/help.rs"
CONFIGURATION = ROOT / "docs/CONFIGURATION.md"


def replace_once(path: Path, old: str, new: str) -> None:
    text = path.read_text(encoding="utf-8")
    count = text.count(old)
    if count != 1:
        raise RuntimeError(
            f"expected exactly one match in {path.relative_to(ROOT)}; found {count}: {old[:120]!r}"
        )
    path.write_text(text.replace(old, new, 1), encoding="utf-8")


replace_once(
    CORE_HELP,
    "Write an optional repo policy file (ripr.toml) and, with --ci github, a non-blocking advisory workflow.",
    "Write an optional repo policy file and, with --ci github, separate advisory and badge workflows.",
)

replace_once(
    CORE_HELP,
    "  --ci github      Also write .github/workflows/ripr.yml with advisory reports and optional SARIF rendering/upload.\n",
    """  --ci github      Also write .github/workflows/ripr.yml for advisory PR evidence
                   and .github/workflows/ripr-badge.yml for scheduled/manual
                   badge refresh pull requests.
""",
)

replace_once(
    CORE_HELP,
    "  --force          Overwrite an existing ripr.toml or generated workflow.\n",
    "  --force          Overwrite an existing ripr.toml or either generated workflow.\n",
)

replace_once(
    CORE_HELP,
    """Generated GitHub workflow:
  - installs ripr and writes a pilot packet plus repo report artifacts
  - uploads report artifacts and writes a reviewer-oriented advisory summary
  - surfaces future PR test guidance reports as non-blocking check annotations
  - renders and uploads diff/repo SARIF only while RIPR_UPLOAD_SARIF is true
  - uses continue-on-error for advisory RIPR work and upload steps
  - does not enable baseline failure policy by default
"#;
""",
    """Generated GitHub workflows:
  .github/workflows/ripr.yml:
    - installs ripr and writes a pilot packet plus repo report artifacts
    - uploads report artifacts and writes a reviewer-oriented advisory summary
    - surfaces future PR test guidance reports as non-blocking check annotations
    - renders and uploads diff/repo SARIF only while RIPR_UPLOAD_SARIF is true
    - uses continue-on-error for advisory RIPR work and upload steps
    - does not enable baseline failure policy by default
  .github/workflows/ripr-badge.yml:
    - runs manually or weekly with the RIPR package version pinned
    - validates native repo evidence and the exact four-field Shields payload
    - retains the native audit artifact and opens a PR changing only badges/ripr.json
    - never pushes directly to the default branch
"#;
""",
)

replace_once(
    HELP_TESTS,
    """        assert!(INIT_HELP.contains("--ci github"));
        assert!(INIT_HELP.contains("--dry-run"));
""",
    """        assert!(INIT_HELP.contains("--ci github"));
        assert!(INIT_HELP.contains(".github/workflows/ripr.yml"));
        assert!(INIT_HELP.contains(".github/workflows/ripr-badge.yml"));
        assert!(INIT_HELP.contains("never pushes directly to the default branch"));
        assert!(INIT_HELP.contains("--dry-run"));
""",
)

replace_once(
    CONFIGURATION,
    "| CI | Generated GitHub workflows upload advisory pilot/report/agent artifacts, keep SARIF rendering/upload optional, and use `continue-on-error` by default. |\n",
    "| CI | Generated GitHub workflows separate advisory PR evidence from scheduled/manual badge publication. PR evidence keeps SARIF optional and uses `continue-on-error`; badge publication validates contracts and opens a narrow PR. |\n",
)

replace_once(
    CONFIGURATION,
    """defaults. With `--ci github`, `ripr init` also writes a non-blocking GitHub
Actions workflow for pilot/report/agent artifacts, optional repo-local cockpit
rendering, and optional SARIF rendering/upload. It does not run mutation
testing, enable CI blocking policy, or unlock basic CLI usefulness.
""",
    """defaults. With `--ci github`, `ripr init` also writes two GitHub Actions
workflows: `.github/workflows/ripr.yml` for non-blocking pilot/report/agent
artifacts, optional repo-local cockpit rendering, and optional SARIF; and
`.github/workflows/ripr-badge.yml` for manual or scheduled badge refreshes that
validate both contracts and open a narrow endpoint PR. It does not run mutation
testing, enable CI blocking policy, or unlock basic CLI usefulness.
""",
)

replace_once(
    CONFIGURATION,
    "| `--ci github` | _(off)_ | Also write `.github/workflows/ripr.yml`. The workflow installs `ripr`, runs `ripr pilot`, uploads pilot/report/agent artifacts, writes repo badge JSON, optionally renders and uploads SARIF when `RIPR_UPLOAD_SARIF` is true, and uses `continue-on-error` so the default path is advisory. |\n",
    "| `--ci github` | _(off)_ | Also write `.github/workflows/ripr.yml` and `.github/workflows/ripr-badge.yml`. The first runs advisory PR evidence and optional SARIF; the second runs manually or weekly, validates native/Shields badge contracts, retains the native audit artifact, and opens a PR changing only `badges/ripr.json`. |\n",
)

replace_once(
    CONFIGURATION,
    "| `--force` | _(off)_ | Overwrite an existing `ripr.toml` or generated workflow. Without this flag, existing repo policy and workflow files are left unchanged. |\n",
    "| `--force` | _(off)_ | Overwrite an existing `ripr.toml` or either generated workflow. Without this flag, an existing workflow blocks the run; an existing `ripr.toml` is left unchanged when `--ci` still has targets to create. |\n",
)

replace_once(
    CONFIGURATION,
    "| the target workflow already exists, no `--force` | with `--ci` |\n",
    "| either target workflow already exists, no `--force` | with `--ci` |\n",
)

replace_once(
    CONFIGURATION,
    """With `--ci`, the run still has a workflow to write, so an existing config is
reported as `leave existing` and the run proceeds — that is the case shown
""",
    """With `--ci`, the run still has two workflows to write, so an existing config
is reported as `leave existing` and the run proceeds — that is the case shown
""",
)

replace_once(
    CONFIGURATION,
    """  leave existing ./ripr.toml
  create         ./.github/workflows/ripr.yml

# ./.github/workflows/ripr.yml
name: RIPR
...

Rerun without --dry-run to apply.
""",
    """  leave existing ./ripr.toml
  create         ./.github/workflows/ripr.yml
  create         ./.github/workflows/ripr-badge.yml

# ./.github/workflows/ripr.yml
name: RIPR
...

# ./.github/workflows/ripr-badge.yml
name: RIPR badge refresh
...

Rerun without --dry-run to apply.
""",
)

print("Aligned issue #2017 CLI help, help assertions, and configuration reference.")
