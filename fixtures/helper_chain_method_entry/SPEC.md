# Fixture: helper_chain_method_entry

Spec: RIPR-SPEC-0159

## Given

A changed private method (`Parser::secs`) reached only through a public entry: `entry -> Parser{..}.total() -> self.secs()`. The exact-value test calls only the public entry.

## When

```bash
cargo xtask fixtures helper_chain_method_entry
```

## Then

The method hops bind by impl identity, the entry's exact assertion pins the owner's returned value through the forwarding chain, and the finding reads `exposed`.

## Must Not

- Credit a variable receiver, another type's same-named method, or a smoke assertion through the entry.
