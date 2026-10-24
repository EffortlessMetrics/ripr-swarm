<!-- section: Fixed -->
- A lone closing brace added outside every function body (the existing `trait`/`impl`/`struct` closer that diff alignment reports as added when new items are appended) no longer seeds a `static_unknown` probe or borrows unrelated tests as related (#10090).
