<!-- section: Fixed -->
- The VS Code extension starts analysis when the project marker is below the
  workspace root (for example `backend/Cargo.toml` or
  `packages/app/tsconfig.json`, up to three folders down) instead of staying
  idle with only an output-channel line (#15916).
