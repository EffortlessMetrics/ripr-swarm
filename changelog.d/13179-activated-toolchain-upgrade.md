<!-- section: Internal / CI -->
- Make the Linux stable-upgrade coverage recipe select Cargo, rustc, and matching LLVM tools in command-scoped subshells so the pin and stable checks work after either toolchain has already been activated. (#13179)
