<!-- section: Fixed -->
- TypeScript related-test discovery now resolves each non-relative import specifier against the workspace alias map and package table once per analysis run instead of once per owner x test x import combination. Resolution results, `ripr check` output and the fail-closed rules are unchanged; outside a run every call still resolves afresh (#10925).
