<!-- section: Fixed -->
- TypeScript preview withholds runner matcher credit from locally declared
  `expect` bindings inside named exports or named default-exported functions
  and classes. No-op lookalikes can no longer promote a changed return to
  `exposed` solely through a `toBe` property (#1235).
