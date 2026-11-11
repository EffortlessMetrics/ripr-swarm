<!-- section: Internal / CI -->
- Stop retrying a runner-inventory request after HTTP 401 while retaining the
  recorded failure, warning, and GitHub-hosted fallback; transient errors still
  use the bounded retry path (#8858).
