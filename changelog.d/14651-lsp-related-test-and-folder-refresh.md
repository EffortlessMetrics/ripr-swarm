<!-- section: Fixed -->
- The language server now also refreshes when a test that a finding lists as a
  related test is rewritten outside the editor (the coding-agent repair loop),
  and when a whole folder holding analyzed files is deleted as one entry.
  Deletions are now watched for every path under the workspace root, so a
  folder removed in one step is reported to the server (#14099).
