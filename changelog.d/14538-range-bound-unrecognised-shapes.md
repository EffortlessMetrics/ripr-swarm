<!-- section: Fixed -->
- More range-bound edits stop reading `exposed` at a test input where both ranges agree: several ranges changed on one line (`(0..=a).sum::<u32>() + (0..=b).sum::<u32>()`), and an endpoint moved inside `matches!(n, 1..=5)`. An assertion inside a nested `fn`, a closure, an `async` block or a loop body no longer supplies the separating input, since it may never run (#14538).
