<!-- section: Fixed -->
- The VS Code extension no longer waits on the "ripr server is not available" popup before finishing startup, so an unanswered popup can no longer hang activation or swallow a later start or Retry, and the server probe budget on Windows is 30 s instead of 10 s because a first run of a fresh `ripr.exe` can exceed 10 s (#9526, #2769).
