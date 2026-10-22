<!-- section: Fixed -->
- The VS Code extension no longer waits on the "ripr server is not available" popup before finishing startup, so an unanswered popup can no longer hang activation or swallow a later start or Retry (#9526, #2769).
- The Windows server probe budget is 30 s instead of 10 s, because a cold first probe was observed to exceed 10 s; why it is slow is tracked in #9631 (#9526).
