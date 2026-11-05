"""Bind a validated history sync to its live authorization and integration.

Read-only companion to candidate_admission.history_sync_check. Its digest is
an invocation-local continuity witness, not product proof or an atomic base
lease. It never admits a different head, publishes a check, or merges a PR.
"""
from __future__ import annotations

import argparse
import hashlib
import json
import re
import sys
from typing import Any, Callable
from urllib.parse import quote

OID = re.compile(r'[0-9a-f]{40}')
DIGEST = re.compile(r'[0-9a-f]{64}')
REPOSITORIES = {'EffortlessMetrics/ripr', 'EffortlessMetrics/ripr-swarm'}


class Refused(ValueError):
    pass


def require(condition: bool, message: str) -> None:
    if not condition:
        raise Refused(message)


def snapshot(api: Callable[..., Any], validator: Callable[..., dict[str, str]],
             repository: str, number: int, head: str, base: str, integration: str,
             source: str, destination_base: str) -> dict[str, Any]:
    """Re-run the canonical validator and bind the actual current GitHub G."""
    require(repository in REPOSITORIES and type(number) is int and number > 0,
            'invalid repository or PR identity')
    require(all(isinstance(s, str) and OID.fullmatch(s)
                for s in (head, base, integration, destination_base)), 'invalid exact commit identity')
    root = f'repos/{repository}'
    pr_path, base_path = f'{root}/pulls/{number}', f'{root}/git/ref/heads/main'
    current = api(pr_path)

    def identity(pr: Any) -> bool:
        if not isinstance(pr, dict):
            return False
        h, b = pr.get('head') or {}, pr.get('base') or {}
        labels = pr.get('labels')
        return (pr.get('number') == number and pr.get('state') == 'open'
                and pr.get('draft') is False and h.get('sha') == head
                and (h.get('repo') or {}).get('full_name') == repository
                and b.get('sha') == base and b.get('ref') == 'main'
                and (b.get('repo') or {}).get('full_name') == repository
                and pr.get('merge_commit_sha') == integration
                and isinstance(labels, list)
                and {'history-sync', 'merge-ready'} <= {
                    label.get('name') for label in labels if isinstance(label, dict)})

    require(identity(current), 'live PR no longer binds the admitted H/B/G')
    ref = api(base_path)
    require(isinstance(ref, dict) and (ref.get('object') or {}).get('sha') == base,
            'destination main moved')
    captured: dict[str, Any] = {}

    def recorded(path: str, **kwargs: Any) -> Any:
        result = api(path, **kwargs)
        captured[path] = result
        return result

    verdict = validator(current, repository, 'pull_request', base, recorded)
    require(isinstance(verdict, dict) and verdict.get('status') == 'validated',
            'canonical history-sync admission no longer validates')
    require(f"{verdict.get('source')}@{verdict.get('source_sha')}" == source
            and verdict.get('destination_base') == destination_base,
            'source declaration changed')
    # Bind the particular event which the canonical validator authorized, not
    # merely label presence. Any removal/reapplication changes this identity.
    events = captured.get(f'{root}/issues/{number}/events?per_page=100')
    require(isinstance(events, list), 'authorized label event was not observed')
    toggles = [e for e in events if isinstance(e, dict)
               and e.get('event') in ('labeled', 'unlabeled')
               and (e.get('label') or {}).get('name') == 'history-sync']
    require(bool(toggles) and all(isinstance(e.get('created_at'), str)
                                 and type(e.get('id')) is int and e['id'] > 0
                                 for e in toggles), 'authorized label event identity is invalid')
    event = max(toggles, key=lambda e: (e['created_at'], e['id']))
    actor = (event.get('actor') or {}).get('login')
    require(event.get('event') == 'labeled' and isinstance(actor, str) and bool(actor),
            'history-sync authorization was withdrawn')
    permission = captured.get(f'{root}/collaborators/{quote(actor)}/permission')
    require(isinstance(permission, dict) and permission.get('role_name') in ('admin', 'maintain'),
            'authorizing actor no longer has the required role')
    commit = api(f'{root}/git/commits/{integration}')
    require(isinstance(commit, dict) and commit.get('sha') == integration
            and isinstance(commit.get('parents'), list)
            and [p.get('sha') for p in commit['parents'] if isinstance(p, dict)] == [base, head]
            and len(commit['parents']) == 2, 'integration parents differ from [B,H]')
    tree = (commit.get('tree') or {}).get('sha')
    require(isinstance(tree, str) and bool(OID.fullmatch(tree)), 'integration tree is unavailable')
    # Reduce the acquisition window. This is readback, not a compare-and-swap
    # lease: protected expected-base enforcement remains independently required.
    reread = api(pr_path)
    ref_after = api(base_path)
    require(identity(reread) and reread.get('body') == current.get('body')
            and isinstance(ref_after, dict)
            and (ref_after.get('object') or {}).get('sha') == base,
            'subject moved while reading its continuity witness')
    return dict(schema='ripr.history_sync_live.v1', repository=repository, pr=number,
                head=head, base=base, integration=integration, tree=tree,
                source=source, destination_base=destination_base,
                label_event=event['id'], label_created_at=event['created_at'],
                label_actor=actor, role=permission['role_name'])


def fingerprint(witness: dict[str, Any]) -> str:
    return hashlib.sha256(json.dumps(witness, sort_keys=True,
                                     separators=(',', ':')).encode()).hexdigest()


def main(argv: list[str] | None = None) -> int:
    p = argparse.ArgumentParser(description=__doc__)
    for name in ('repository', 'head', 'base', 'integration', 'source', 'destination-base'):
        p.add_argument('--' + name, required=True)
    p.add_argument('--pr', required=True, type=int)
    p.add_argument('--expect-digest')
    args = p.parse_args(argv)
    try:
        # Imported from the same pinned trusted bundle as this helper. The
        # existing validator remains the sole source/ancestry admission policy.
        from candidate_admission import gh_api_json, history_sync_check
        observed = fingerprint(snapshot(gh_api_json, history_sync_check,
                                        args.repository, args.pr, args.head, args.base,
                                        args.integration, args.source, args.destination_base))
        if args.expect_digest is not None:
            require(bool(DIGEST.fullmatch(args.expect_digest))
                    and observed == args.expect_digest,
                    'authorization or composition changed after admission')
        print(f'digest={observed}')
        return 0
    except (Refused, OSError, ValueError, TypeError, KeyError, AttributeError) as error:
        print(f'history-sync continuity refused: {error}', file=sys.stderr)
        return 1


if __name__ == '__main__':
    raise SystemExit(main())
