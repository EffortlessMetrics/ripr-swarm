<!-- section: Security -->
- The Windows zip pre-extraction check for VS Code managed-server downloads now also refuses `:` stream names, empty names, trailing dot/space segments, and reserved device names (including `COM¹`-style, `CONIN$` and `CONOUT$` aliases). It also refuses ZIP64 records, split entry counts, and an end-of-central-directory record whose comment does not end the file (#13904).
