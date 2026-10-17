<!-- section: Fixed -->
- PowerShell translation of a Windows console-form command no longer rewrites
  bash's apostrophe-escape sequence inside double quotes, so a path whose
  component boundary is an apostrophe then a separator then apostrophes keeps
  that directory separator (#8260).
