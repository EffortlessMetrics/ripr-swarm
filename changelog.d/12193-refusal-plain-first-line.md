<!-- section: Changed -->
- A `ripr repair`, `ripr continue` or `ripr agent repair` refusal now opens its stderr with the plain-words reason and next step, and prints the machine line `ripr: refusal code=<code> disposition=<disposition>` on its own later line instead of first; the line, the exit codes and the JSON `code` field are unchanged (#12193).
