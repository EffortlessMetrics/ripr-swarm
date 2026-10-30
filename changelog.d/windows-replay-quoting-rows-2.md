<!-- section: Internal / CI -->
- Windows test lane: the agent-status replay rows expect the host's printed
  quoting form, and the Windows replay splitter keeps a recovery hint's
  `<ref>` placeholder as one word, as the POSIX splitter does (#9659, #9272).
