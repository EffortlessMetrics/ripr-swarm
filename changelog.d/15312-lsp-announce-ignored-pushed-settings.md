<!-- section: Fixed -->
- Editors that push their settings to the language server now get a warning
  when ripr ignores a pushed value: the log and an on-screen message name the
  setting, its accepted values and the value received. A second typo in the
  same setting warns again, and a payload that is not an object of ripr
  settings is reported instead of dropped silently (#15312).
