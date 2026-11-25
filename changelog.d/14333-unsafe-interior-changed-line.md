<!-- section: Fixed -->
- `ripr check`: a changed line inside an `unsafe fn` or `unsafe { }` block opened on an earlier line now shows the edited line as `Changed behavior` (for example `let repr = unsafe { repr.read() };`) instead of the enclosing header (`unsafe fn inline_len`). The finding, its id and gap id, and the unsafe-obligation next step are unchanged (#14333).
