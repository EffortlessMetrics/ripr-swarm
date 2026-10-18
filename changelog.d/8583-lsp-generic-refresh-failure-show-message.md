<!-- section: Fixed -->
- Generic LSP clients now receive one bounded `window/showMessage` warning
  when analysis refresh first fails, instead of only a log line and a
  custom `ripr/analysisStatus` notification they do not render. Repeated
  failures stay silent until a successful refresh; the VS Code path is
  unchanged (#8583).
