<!-- section: Fixed -->
- `ripr agent verify` and the repair-after phase now classify the no-movement refusal from a typed `AgentVerifyMovementError` variant instead of a message prefix, so rewording the refusal text can no longer flip the exit class or the retry-budget narration (#14021).
