<!-- section: Fixed -->
- TypeScript: a message-only `throw` change on a later line of a multi-line replacement now reads its own old line, so a message check that passes on both messages (chai `to.throw("blank")` for `"label must not be blank"` → `"label is blank"`) reads weak instead of `exposed`. Before, only the first added line of a replaced block found its old line (#7524).
