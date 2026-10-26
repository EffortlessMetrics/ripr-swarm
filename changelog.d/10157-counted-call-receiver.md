<!-- section: Fixed -->
- TypeScript counted call removals receive additional exposure credit only when
  a stable receiver and ordered spy/count witness are available (#10157).
- Counted call-removal witnesses withhold credit for unusable mocked methods,
  unbound or later-initialized deleted arguments, and multi-edit files (#10466).
