# Install receipt

R0 install row: **not_met** (matrix run 41902877159)

| channel | platform | status | version | install seconds |
| --- | --- | --- | --- | ---: |
| cargo | macos-15-intel | works | ripr 0.10.0 | 729 |
| cargo | macos-latest | works | ripr 0.10.0 | 182 |
| cargo | ubuntu-24.04-arm | works | ripr 0.10.0 | 91 |
| cargo | ubuntu-latest | works | ripr 0.10.0 | 101 |
| cargo | windows-latest | works | ripr 0.10.0 | 243 |
| npm | macos-15-intel | known_gap | - | - |
| npm | macos-latest | known_gap | - | - |
| npm | ubuntu-24.04-arm | known_gap | - | - |
| npm | ubuntu-latest | works | ripr 0.11.0-alpha.2 | 3 |
| npm | windows-latest | known_gap | - | - |
| pypi | macos-15-intel | known_gap | - | - |
| pypi | macos-latest | known_gap | - | - |
| pypi | ubuntu-24.04-arm | known_gap | - | - |
| pypi | ubuntu-latest | works | ripr 0.11.0-alpha.1 | 4 |
| pypi | windows-latest | known_gap | - | - |

| other channel | disposition | reason |
| --- | --- | --- |
| archive | tested_elsewhere | Install seconds and first-run walk per platform come from first-run-archive.yml (weekly and on demand), whose report artifact records them; five platforms were green on 2026-11-04 (run 41882613579). |
| binstall | not_tested | cargo binstall ripr falls back to a source build until a release carries binstall metadata and archives (0.11.0); verify with a real cargo binstall after publication. |
| vscode_extension | not_tested | Marketplace and Open VSX install needs a desktop VS Code host; no hosted check installs the listed extension and starts the server. |
| ci_init | not_tested | The generated GitHub workflow downloads the release archive when one exists for the runner; no scheduled check runs a generated workflow on a clean repository. |
| homebrew | not_shipped | No tap exists; creating it needs the owner's go-ahead (docs/INSTALL_CHANNELS.md). |

Why not met:

- cargo on macos-15-intel took 729 s, over 300 s
- npm on macos-15-intel is an unavailable platform
- npm on macos-latest is an unavailable platform
- npm on ubuntu-24.04-arm is an unavailable platform
- npm on windows-latest is an unavailable platform
- pypi on macos-15-intel is an unavailable platform
- pypi on macos-latest is an unavailable platform
- pypi on ubuntu-24.04-arm is an unavailable platform
- pypi on windows-latest is an unavailable platform
- channel binstall is not tested
- channel vscode_extension is not tested
- channel ci_init is not tested
