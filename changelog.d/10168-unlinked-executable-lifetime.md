<!-- section: Internal / CI -->
- The Linux deleted-executable recovery test uses a private lifetime-bound link and reaps its child before cleanup, avoiding writable-image ETXTBSY races without launch retries (#10168).
