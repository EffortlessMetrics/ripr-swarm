<!-- section: Fixed -->
- Retained repair attempts with ambiguous or unavailable before baselines now
  stay limited across status, ordinary repair continuation, VS Code, and MCP.
  Their identity and history remain inspectable; the shared repair gate controls
  clean-state recovery, and limited attempts no longer advertise an after step
  or get reused as ready for editing (#13216).
