<!-- section: Fixed -->
- Editors that push their settings to the language server now get a warning
  when ripr ignores a pushed value: the log names the setting, its accepted
  values and the value received, and editors without ripr's own editor
  integration also show it on screen. A second typo in the same setting warns
  again, and a payload or `ripr` section that is not an object of ripr
  settings is reported instead of dropped silently (#15312).
