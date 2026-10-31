<!-- section: Changed -->
- Python reach admission no longer copies the whole needle set for every pytest fixture host it scans. On Home Assistant (about 20,000 Python files, 3-file diff) a warm draft check drops from 17.95 s to 3.50 s wall and from 65.9 s to 8.6 s CPU at the same 487 MiB peak, with byte-identical findings (#11348).
