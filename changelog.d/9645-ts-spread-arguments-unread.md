<!-- section: Fixed -->
- A TypeScript or JavaScript owner call with a spread argument (`owner(...xs, 4, 5)`) no longer binds the arguments after the spread to parameters by position, so the rounding-mode and other position-reading guards keep their verdict instead of withdrawing a discriminating credit (#9645).
