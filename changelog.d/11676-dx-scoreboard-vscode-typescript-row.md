<!-- section: Added -->
- DX scoreboard: add the heavy `vscode-typescript` row, a three-file edit in VS Code's own tree (5,328 files), with a warm-check gate of 90 s and 1 GiB. Before the TypeScript fact store wrote each shared list once (#11676) every run took about 250 s and 1.47 GiB; the gate sits above today's measured 48.6 s / 457 MiB and is to be tightened as the steady-state costs fall.
