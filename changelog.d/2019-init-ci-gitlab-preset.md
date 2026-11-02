<!-- section: Added -->
- `ripr init --ci gitlab` writes an advisory merge-request `.gitlab-ci.yml`
  that installs ripr, runs `ripr check` and `ripr review-comments`, and
  uploads `target/ripr/**` job artifacts (#2019).
