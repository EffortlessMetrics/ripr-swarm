<!-- section: Fixed -->
- Successful `ripr init` uses a lossless supplied or resolved filesystem root
  for executable follow-ups, including UTF-8 parents reached from byte-invalid
  directories. If neither spelling is lossless, it preserves completed writes,
  omits the commands, and names the UTF-8-parent/alias remedy (#7830).
