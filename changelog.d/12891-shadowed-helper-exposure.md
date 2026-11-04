<!-- section: Fixed -->
- Repository exposure no longer credits a same-named module helper when a
  function type parameter shadows its call path; unresolved competing paths stay
  unknown, while established module and imported calls keep their credit (#12891).
