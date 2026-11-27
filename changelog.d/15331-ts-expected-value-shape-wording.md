<!-- section: Fixed -->
- A TypeScript test whose expected value ripr cannot resolve is now described by
  its shape: `undefined` is no longer called "a variable or computed value", and
  an object or array literal holding variables says it contains variables and
  advises pinning the member values. Classification and actionability are
  unchanged; only the wording differs (#15331).
