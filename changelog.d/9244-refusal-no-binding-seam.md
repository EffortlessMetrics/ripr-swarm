<!-- section: Changed -->
- `ripr check` no longer answers a finding that no seam binds to with a pointer back at the gate. The line now says ripr found no seam in the finding's file to bind it to, so there is no seam to start a repair from, and sends the reader to the Next step above. The classification-failed refusal now says the gate may print the underlying error (#9244).
