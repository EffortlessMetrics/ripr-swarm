<!-- section: Fixed -->
- Repo-wide classified runs also prepare the complete compact summary cache when inputs fit the prefill limits, so a following `repo-exposure-summary-json` can reuse it. A declined or failed prefill preserves the prior cache and the full analysis result; changed analysis inputs still refuse the run (#12506).
