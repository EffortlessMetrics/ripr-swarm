<!-- section: Fixed -->
- Editor diagnostics no longer search a whole bare-CR (`\r`-only) file as one saved line; the analysis and the editor count lines differently there, so the range falls back to the finding's numeric location instead of a misplaced span (#8589).
