# Fixture: python_adversarial_attribute_sink_same_value_other_receiver (false-exposed guard — attribute receiver identity, same value)

Spec: RIPR-SPEC-0028

## Given

An adversarial **attribute changed-sink, same-value different-receiver**
over-credit trap. The changed owner `Session.refresh` (src/session.py) flips
an attribute write `self.status = "idle"` → `self.status = "active"`. The only
related test reaches the owner but its strong exact-value oracle asserts the
**same value** on a **different object's** same-named attribute:

```python
# changed owner: Session.refresh
self.status = "active"

from src.session import Session
from src.conn import Conn

def test_session_refresh():
    Session().refresh()
    conn = Conn()
    assert conn.status == "active"
```

`conn` is a `Conn`, not the `Session` instance `refresh` ran on (the test uses
an inline `Session()` with no bound local). Observing `"active"` and the token
`status` on a foreign receiver is token coincidence, not observation of the
changed sink.

## When

`ripr check` analyzes the diff against the Python preview adapter.

## Then

ripr does **not** classify the change `exposed`; the oracle alignment is not
`changed_sink_token`. For an attribute-assignment changed sink on a method
owner, the value+attribute co-observation path credits only when the asserted
attribute receiver is bound to the owner: the same instance the owner method
ran on (a local bound to an owner construction and called with the method, or
a setup/fixture receiver bound likewise).

**This fixture must NEVER read `exposed`.** Before the receiver-identity gate
it read `exposed`/`strong_oracle_observes_changed_sink_token` (verified red).
The companion `python_field_assignment_shape` fixture pins the inverse: when
the oracle observes the assigned value `"paid"` on the changed field of the
bound instance (`invoice.status`), the change stays `exposed`. The companion
`python_adversarial_attribute_sink_other_receiver` pins the different-value
variant.

## Must Not

- Credit `exposed` from value+attribute token matches on a receiver not bound
  to the changed owner.
- Run any Python runtime; static preview evidence only.
