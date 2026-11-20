<!-- section: Fixed -->
- `ripr agent repair --phase before` admits the Git-ignored build directory again immediately before its first shared workflow write, so an ignore-rule change after the early admission refuses as not started instead of at the late attempt-publication recheck after shared workflow files were replaced (#13452).
