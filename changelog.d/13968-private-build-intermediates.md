<!-- section: Internal / CI -->
- Private native-libtest and fixture builds bind Cargo intermediate output to
  their existing private target directory, overriding inherited and configured
  build-directory redirects while preserving Cargo command lines and libtest
  receipt compatibility (#13968).
