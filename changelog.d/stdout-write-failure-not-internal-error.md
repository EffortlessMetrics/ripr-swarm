<!-- section: Fixed -->
- A stdout that fails for a reason other than a closed pipe (for example `ripr --version > /dev/full` or a full disk behind a report redirect) now reports `write to stdout failed: <cause>` with a next step and exit 2, instead of "internal error (this is a bug)" and a backtrace (#15369).
