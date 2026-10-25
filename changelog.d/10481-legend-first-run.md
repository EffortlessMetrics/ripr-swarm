<!-- section: Changed -->
- `ripr check` prints the ten-line Evidence legend on the first run in a build tree and keeps one `Terms:` pointer line afterwards, so repeat runs put the finding's reasons directly under the Evidence line. `ripr check --legend` prints the definitions again; other formats and library callers are unchanged (#10481).
