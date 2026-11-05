<!-- section: Internal / CI -->
- The Unix interrupt test harness now exercises an inherited ignored SIGINT and requires both the existing child's started marker and native SIGINT termination, guarding the default-handler restoration against a bare-raise regression (#12887).
