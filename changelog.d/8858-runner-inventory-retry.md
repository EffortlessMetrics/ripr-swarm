<!-- section: Fixed -->
- The routed Rust router retries the runner inventory query up to three times with short backoff before recording `runner_api_failed`, and reports the attempt count and any digit-only HTTP status in the step summary without raw stderr (#8858).
