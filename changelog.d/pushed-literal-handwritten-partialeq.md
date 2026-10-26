<!-- section: Fixed -->
- A field literal pushed onto a collection no longer reads `exposed` from a whole-collection comparison when the pushed element type has a hand-written `PartialEq`, or embeds a type that does: its `eq` may ignore the changed field. A literal whose type has no workspace `struct` or `enum` definition fails closed the same way; derived `PartialEq` still confirms (#7697).
