# Install receipt

R0 install row: **not_established** (runs 41902877159, 42481871664, 42481874498, 42481881516)

Prebuilt channels are judged against 300 s. `cargo install` builds from source and is judged on the median of at least 3 samples against 900 s.

| channel | build | platform | status | version | median seconds | samples |
| --- | --- | --- | --- | --- | ---: | --- |
| archive | prebuilt | macos-15-intel | works | ripr 0.10.0 | 1 | 1 |
| archive | prebuilt | macos-latest | works | ripr 0.10.0 | 1 | 1 |
| archive | prebuilt | ubuntu-24.04-arm | works | ripr 0.10.0 | 1 | 1 |
| archive | prebuilt | ubuntu-latest | works | ripr 0.10.0 | 0 | 0 |
| archive | prebuilt | windows-latest | works | ripr 0.10.0 | 2 | 2 |
| binstall | prebuilt | macos-15-intel | known_gap | - | - | known_gap, known_gap |
| binstall | prebuilt | macos-latest | known_gap | - | - | known_gap, known_gap |
| binstall | prebuilt | ubuntu-24.04-arm | known_gap | - | - | known_gap, known_gap |
| binstall | prebuilt | ubuntu-latest | known_gap | - | - | known_gap, known_gap |
| binstall | prebuilt | windows-latest | known_gap | - | - | known_gap, known_gap |
| cargo | source | macos-15-intel | works | ripr 0.10.0 | 334 | 729, 307, 334 |
| cargo | source | macos-latest | works | ripr 0.10.0 | 182 | 182, 112, 185 |
| cargo | source | ubuntu-24.04-arm | works | ripr 0.10.0 | 93 | 91, 93, 94 |
| cargo | source | ubuntu-latest | works | ripr 0.10.0 | 101 | 101, 98, 120 |
| cargo | source | windows-latest | works | ripr 0.10.0 | 243 | 243, 254, 150 |
| npm | prebuilt | macos-15-intel | known_gap | - | - | known_gap, known_gap, known_gap |
| npm | prebuilt | macos-latest | known_gap | - | - | known_gap, known_gap, known_gap |
| npm | prebuilt | ubuntu-24.04-arm | known_gap | - | - | known_gap, known_gap, known_gap |
| npm | prebuilt | ubuntu-latest | works | ripr 0.11.0-alpha.2 | 3 | 3, 4, 2 |
| npm | prebuilt | windows-latest | known_gap | - | - | known_gap, known_gap, known_gap |
| pypi | prebuilt | macos-15-intel | known_gap | - | - | known_gap, known_gap, known_gap |
| pypi | prebuilt | macos-latest | known_gap | - | - | known_gap, known_gap, known_gap |
| pypi | prebuilt | ubuntu-24.04-arm | known_gap | - | - | known_gap, known_gap, known_gap |
| pypi | prebuilt | ubuntu-latest | works | ripr 0.11.0-alpha.1 | 4 | 4, 4, 4 |
| pypi | prebuilt | windows-latest | known_gap | - | - | known_gap, known_gap, known_gap |

| other channel | disposition | reason |
| --- | --- | --- |
| vscode_extension | not_tested | Marketplace and Open VSX install needs a desktop VS Code host; no hosted check installs the listed extension and starts the server. |
| ci_init | not_tested | The generated GitHub workflow downloads the release archive when one exists for the runner; no scheduled check runs a generated workflow on a clean repository. |
| homebrew | not_shipped | No tap exists; creating it needs the owner's go-ahead (docs/INSTALL_CHANNELS.md). |

Why not established:

- binstall on macos-15-intel is an unavailable platform
- binstall on macos-latest is an unavailable platform
- binstall on ubuntu-24.04-arm is an unavailable platform
- binstall on ubuntu-latest is an unavailable platform
- binstall on windows-latest is an unavailable platform
- npm on macos-15-intel is an unavailable platform
- npm on macos-latest is an unavailable platform
- npm on ubuntu-24.04-arm is an unavailable platform
- npm on windows-latest is an unavailable platform
- pypi on macos-15-intel is an unavailable platform
- pypi on macos-latest is an unavailable platform
- pypi on ubuntu-24.04-arm is an unavailable platform
- pypi on windows-latest is an unavailable platform
- channel vscode_extension is not tested
- channel ci_init is not tested
