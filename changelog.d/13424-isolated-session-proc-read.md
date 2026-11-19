<!-- section: Internal / CI -->
- The Linux isolated-test session reader uses the shared procfs disappearance
  classifier for ENOENT and ESRCH, so a task exiting during inspection does not
  interrupt cleanup. Other read errors and malformed process records remain
  failures (#13424).
