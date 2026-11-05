<!-- section: Internal / CI -->
- Coverage diagnostics read LLVM raw profile v10 and v11 binary IDs using the
  correct header sizes, retain explicit unsupported and partial results, and
  compile with stable Clippy while preserving Rust 1.95 compatibility. A focused
  advisory stable-toolchain job checks real profile generation and matching LLVM
  tools and isolated workflow fixtures before future upgrades. Historical
  profiles remain captured when newer LLVM reports a raw profile version
  mismatch (#13116).
