<!-- section: Fixed -->
- The routed Rust router now emits a distinct warning when the runner inventory query is rejected with HTTP 401 or 403 (token expired or missing scope) while still falling back to the hosted leg, and the Result's eligibility recheck explains a re-run of an earlier attempt's request instead of failing silently. Routing and refusal conditions are unchanged (#8858).
