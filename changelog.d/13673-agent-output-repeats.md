<!-- section: Changed -->
- `ripr check`: hidden rows that can start a test now print their test id plus one shared `ripr agent stub ... --seam-id TEST_ID` command instead of a full command per row (#13673).
- `ripr agent brief --json`: hidden seams that share an omission reason share one warning sentence (`seams A at f:l, B at f:l are ...`); a single hidden seam keeps its wording (#13673).
