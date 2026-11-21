<!-- section: Fixed -->
- A unit function's tail `unsafe { ...; }` or plain block ending in a `;`-terminated statement no longer gets a `return_value` probe; its statements keep their `side_effect`/`call_deletion` probes (#15459)
