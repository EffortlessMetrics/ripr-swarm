# Fixture: fixture_receiver_extension_iter

Spec: RIPR-SPEC-0240

## Given

A helper returns two literal cart items. A local extension trait on the standard
Vec, emitted through associated trait/impl macros inside raw module `r#owner`,
supplies `iter()` and returns an empty slice iterator before deref can reach
the ordinary slice method. The exact assertion pins the total to zero.

## When

The whole returned projection sum changes its tuple binding name and ripr reads
the same-file fixture receiver.

## Then

The finding remains `static_unknown` with the framework-oracle limit. The
nonempty vector does not establish that the projected closure executes.

## Must Not

- Promote this finding to exposed from the readable literal vector.
- Treat method spelling as established standard iterator dispatch.
