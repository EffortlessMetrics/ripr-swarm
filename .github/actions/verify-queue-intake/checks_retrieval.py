"""Bounded exact-H Checks retrieval for the source-only intake validator.

The injected transport must authenticate the repository API, enforce TLS and
timeouts, and report the provider's next page without following redirects to
untrusted origins. This module does not supply credentials, authenticate the
transport or establish final qualification. It is intentionally unwired.
"""

from copy import deepcopy
from dataclasses import dataclass
import json

import queue_intake as intake


@dataclass(frozen=True)
class EnqueueAcknowledgement:
    """Durably retained response to the expected-H native enqueue operation.

    This must come from the trusted operation receipt, never be reconstructed
    from a visible queue entry or the check that this module reads.
    """

    repository: str
    pull_number: int
    head_sha: str
    entry_id: str
    admission_request_id: str
    enqueue_request_id: str


@dataclass(frozen=True)
class Page:
    """HTTP result; next_page is the provider's parsed next-link page number.

    The transport must reject malformed, cross-origin or altered-query links;
    None means the response had no next link. Body must be the raw JSON bytes.
    """

    status: int
    body: bytes
    next_page: int | None


def _acknowledgement(binding, ack):
    intake.require(isinstance(ack, EnqueueAcknowledgement), "enqueue acknowledgement is missing")
    intake.require(ack.repository == binding.repository and type(ack.pull_number) is int
                   and ack.pull_number == binding.pull_number and ack.head_sha == binding.head_sha
                   and ack.entry_id == binding.entry_id, "acknowledgement identity differs")
    for value in (ack.admission_request_id, ack.enqueue_request_id):
        intake.require(isinstance(value, str) and intake.NAME.fullmatch(value) is not None,
                       "acknowledgement request identity is missing")


def _checks(transport, repository, path):
    """Read every page, refusing inconsistent, duplicated or truncated sets."""
    rows = []
    ids = set()
    total = None
    received_bytes = 0
    for page_number in range(1, 101):
        try:
            page = transport.get(repository, f"{path}&page={page_number}")
        except Exception as error:
            raise intake.Refused("Checks transport failed") from error
        intake.require(isinstance(page, Page) and type(page.status) is int and page.status == 200
                       and isinstance(page.body, bytes), "Checks response is unavailable")
        received_bytes += len(page.body)
        intake.require(received_bytes <= intake.MAX_BYTES, "Checks pages exceed their byte bound")
        try:
            data = intake.decode(page.body)
        except RecursionError as error:
            raise intake.Refused("Checks JSON nesting exceeds its bound") from error
        intake.require(isinstance(data, dict) and type(data.get("total_count")) is int
                       and 0 <= data["total_count"] <= 100
                       and isinstance(data.get("check_runs"), list),
                       "bounded Checks result is unavailable")
        if total is None:
            total = data["total_count"]
        intake.require(data["total_count"] == total, "Checks total changed between pages")
        batch = data["check_runs"]
        intake.require(len(rows) + len(batch) <= total, "Checks result exceeds declared total")
        for check in batch:
            intake.require(isinstance(check, dict) and intake.positive(check.get("id"))
                           and check["id"] not in ids, "missing or duplicate check identity")
            ids.add(check["id"])
            rows.append(check)
        if page.next_page is None:
            intake.require(len(rows) == total, "Checks pagination is truncated")
            # The shared validator owns all producer and v2 intake semantics.
            return json.dumps({"total_count": total, "check_runs": sorted(rows, key=lambda r: r["id"])},
                              allow_nan=False, sort_keys=True)
        intake.require(type(page.next_page) is int and page.next_page == page_number + 1
                       and bool(batch) and len(rows) < total, "invalid Checks pagination")
    raise intake.Refused("Checks pagination exceeds its bound")


def observe(binding, acknowledgement, transport, read_current, now, between_reads):
    """Read twice around metadata and live-entry observations, with no authority grant.

    The caller owns the trusted config, durable acknowledgement, current-entry
    reader, clock and authenticated transport. A valid result is only an H
    observation, not proof of transport identity, actual G, or qualification.
    """
    binding.validate()
    _acknowledgement(binding, acknowledgement)
    saved_ack = deepcopy(acknowledgement)
    saved_binding = deepcopy(binding)
    snapshots = []

    def read_checks(path):
        snapshot = _checks(transport, saved_binding.repository, path)
        intake.require(not snapshots or snapshot == snapshots[0],
                       "complete Checks result changed between observations")
        snapshots.append(snapshot)
        return snapshot

    try:
        result = intake.observe(binding, read_checks, read_current, now, between_reads)
    except RecursionError as error:
        raise intake.Refused("intake observation nesting exceeds its bound") from error
    intake.require(binding == saved_binding and acknowledgement == saved_ack,
                   "configuration or acknowledgement changed during observation")
    data = result["intake"]
    intake.require(data["admission_request_id"] == saved_ack.admission_request_id
                   and data["enqueue_request_id"] == saved_ack.enqueue_request_id,
                   "intake does not match the acknowledged enqueue")
    return {**result, "authorization": "NotEstablished", "qualification": "NotEstablished"}
