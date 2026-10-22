<!-- section: Internal / CI -->
- Windows tests now replay printed `ripr` commands in the form Windows prints
  them: one shared splitter decodes the console double-quote form with the
  native argv rules, and spelling expectations, the `<cwd>` placeholder
  projection and the PowerShell injection control follow the host form. The
  printed commands themselves are unchanged (#9659, #9272).
