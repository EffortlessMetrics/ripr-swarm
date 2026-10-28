# Fixture: typescript_reexport_unparsed_star

Spec: RIPR-SPEC-0095

Related honesty contract: RIPR-SPEC-0108.

## Given

A changed `price` return is imported through a barrel with two star sources.
The owner source parses, but the other source has a parser error, so its
exported names are unavailable.

## When

`cargo xtask fixtures typescript_reexport_unparsed_star` analyzes the return edit.

## Then

The finding stays `no_static_path` with zero related tests. A missing fact
cannot establish that the other star source exports no competing `price`.
The honesty corpus independently rejects an `exposed` golden.

## Controls

The CLI regression also starts with valid sources and lowers only the file
read cap. A complete ambiguous graph and its capped version both withhold
credit; a parsed empty sibling permits credit, and losing its facts withholds
it. Restoring readable, valid source recovers the supported positive case.

## Must Not

- Treat a parser-error module as successfully parsed empty exports.
- Turn a refused star sibling into positive owner or namespace evidence.
- Change explicit-export precedence or direct-import handling.
