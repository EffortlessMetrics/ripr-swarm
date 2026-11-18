<!-- section: Fixed -->
- Python: a changed string on a line inside an open bracket (an exception message split across lines, a list element, a call argument) is no longer dropped as a docstring-style no-op, so ripr now reports a finding for it instead of staying silent (#13875).
