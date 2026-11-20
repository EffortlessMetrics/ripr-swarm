<!-- section: Fixed -->
- `ripr reports ts-limitations` and `ripr reports ts-false-actionable` show
  imported paths, ids, limitation kinds and risk classes as literal text in
  their Markdown reports. A Next.js parallel-route path such as
  `app/@modal/page.tsx` no longer notifies an account when the report is
  posted, a generic such as `Array<T>` no longer parses as raw HTML, and a
  backtick in a value no longer breaks its code span (#15042).
