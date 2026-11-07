<!-- section: Fixed -->
- TypeScript committed snapshots withhold strong credit when an exported runner
  config depends on an unread merge or is changed through a variable member key.
  Plain configs, known `defineConfig` imports, and unrelated nested data retain
  their existing treatment (#9940).
