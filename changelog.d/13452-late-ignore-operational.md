<!-- section: Fixed -->
- The attempt-publication recheck of the Git-ignored build directory in `ripr agent repair --phase before` runs after shared workflow files were already written, so a drift it finds is now reported as an operational failure that says those files may have been rewritten, instead of a clean `not_started` refusal (#13452).
