<!-- section: Changed -->
- `ripr pilot` language routes now resolve effective enablement and remediation through the shared `LanguageAvailabilityReport`, matching `ripr doctor --json` instead of re-deriving availability from `is_available` and the enabled list alone; a disabled language keeps its enable step visible in the terminal next to the check route (#2961).
