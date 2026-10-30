# Fixture: python_trailing_whitespace_in_multiline_string (spacing inside a string is an edit)

Spec: RIPR-SPEC-0028

## Given

A Python diff that changes only the spacing between two words on a line that sits
inside a triple-quoted string:

```python
def build_query():
    sql = """
    SELECT a  FROM t     # was: "SELECT a FROM t"
    WHERE b = 1
    """
    return sql
```

Each lexed alone the old and new line look like the same tokens, but the line is
string content, so the string's value changed.

## When

`ripr check` analyzes the diff against the Python preview adapter.

## Then

ripr does not treat the edit as a spacing-only no-op (#10706): the changed line
still reaches classification and yields a finding.

## Must Not

- Drop a finding for a change inside a multi-line string because the line lexes
  as spaced tokens.
- Run any Python runtime; static preview evidence only.
