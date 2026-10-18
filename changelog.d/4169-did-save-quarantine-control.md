<!-- section: Internal / CI -->
- Hardened the didSave-without-text control for the disowned-buffer quarantine (#4169): the retained buffer now diverges from the persisted bytes so a fallback to buffer text is caught, and the persisted-file re-analysis request is asserted.
