<!-- section: Fixed -->
- Analysis: a crate-qualified constructor receiver (`krate::Rng::new().digit(..)`) no longer credits a changed method of a same-named type in another library module; the path keeps its direct owner call only when the library declares one type of that name (#14083).
