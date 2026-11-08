<!-- section: Internal / CI -->
- Add explicit Linux developer provisioning from the checkout's requirements,
  including the pinned and stable Rust SDKs, source and LLVM components,
  Nextest, locked dependencies and native editor prerequisites, with repeat
  provisioning after pin or lockfile changes and clear native setup failures
  (#13437).
