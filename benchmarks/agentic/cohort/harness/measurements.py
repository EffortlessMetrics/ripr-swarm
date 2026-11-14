"""Receipt measurements: unknown is not zero; SDK cost is not billed cost."""
import hashlib
import json
import math

TOKEN_FIELDS = ("input_tokens", "output_tokens", "cache_creation_input_tokens",
                "cache_read_input_tokens")


def nonnegative(value, *, integer=False):
    """Return a finite measurement, rejecting bools and malformed counters."""
    if type(value) not in (int, float) or (integer and type(value) is not int):
        return None
    if value < 0 or (isinstance(value, float) and not math.isfinite(value)):
        return None
    return value


def terminal_usage(final: dict) -> dict:
    """Read cumulative usage once, never sum repeated assistant messages."""
    observed = final.get("type") == "result"
    raw = final.get("usage") if observed else None
    usage = {key: nonnegative(raw.get(key), integer=True) if isinstance(raw, dict)
             else None for key in TOKEN_FIELDS}
    complete = all(value is not None for value in usage.values())
    return dict(result_observed=observed, result_subtype=final.get("subtype"),
                usage=usage, usage_status="complete" if complete else "not_measured",
                tokens=sum(usage.values()) if complete else None,
                cost_usd=nonnegative(final.get("total_cost_usd")) if observed else None,
                cost_source="sdk_estimate" if observed else None)


def completed(row: dict) -> bool:
    return (type(row.get("rc")) is int and row["rc"] == 0
            and row.get("is_error") is False and not row.get("error")
            and row.get("result_observed", True) is True
            and row.get("result_subtype") in (None, "success"))


def token_count(row: dict):
    if "usage" in row:
        usage = row["usage"]
        if not isinstance(usage, dict) or row.get("result_observed") is not True:
            return None
        counts = [nonnegative(usage.get(key), integer=True) for key in TOKEN_FIELDS]
        return sum(counts) if all(v is not None for v in counts) else None
    # Legacy receipts cannot recover the component breakdown. In particular,
    # their zero fallback on missing terminal results is not a measurement.
    value = nonnegative(row.get("tokens"), integer=True)
    return None if value == 0 and not completed(row) else value


def digest(value) -> str:
    encoded = json.dumps(value, sort_keys=True, separators=(",", ":"),
                         allow_nan=False).encode("utf-8")
    return hashlib.sha256(encoded).hexdigest()
