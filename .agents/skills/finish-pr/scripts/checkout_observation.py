"""Read one PR job's checkout observation; never authorize a merge.

The caller must independently review the exact workflow blobs before supplying
those hashes. GitHub metadata identifies the running source; the observation is
not an authority for its own producer.
"""
import argparse
import datetime
import json
import re
import subprocess
import sys

REPO = 'EffortlessMetrics/ripr-swarm'
REPO_ID = 1243918801
WORKFLOW_ID = 279777287
SHA = re.compile(r'[0-9a-f]{40}\Z')
GATE = re.compile(r'Ripr Rust Small on (?:CX43|CPX42|CX53|GitHub Hosted) / Rust gates\Z')
SHARD = re.compile(r'Ripr Rust Small tests on GitHub Hosted / Rust tests shard [1-4] of 4\Z')
SOURCE = re.compile(r'Checkout source v1 commit=([0-9a-f]{40}) tree=([0-9a-f]{40})\Z')
PARENTS = re.compile(r'Checkout parents v1 ([0-9a-f]{40}(?:,[0-9a-f]{40}){0,2}|none)\Z')
ORIGIN = re.compile(r'Checkout origin v1 repo=([^ ]+) workflow=([0-9a-f]{40}) run=([0-9]+) attempt=([0-9]+) job=([^ ]+)\Z')


class Refusal(Exception):
    pass


def latest_request(api, head, run_id, attempt):
    pages = api.pages(f'actions/workflows/routed-rust.yml/runs?event=pull_request&head_sha={head}&per_page=100')
    if not isinstance(pages, list) or not pages:
        raise Refusal('workflow request inventory unavailable')
    runs, counts = [], []
    for page in pages:
        if not isinstance(page, dict) or type(page.get('total_count')) is not int or not isinstance(page.get('workflow_runs'), list):
            raise Refusal('workflow request inventory incomplete')
        counts.append(page['total_count'])
        runs.extend(page['workflow_runs'])
    if len(set(counts)) != 1 or counts[0] != len(runs) or counts[0] >= 1000:
        raise Refusal('workflow request inventory incomplete, changed or capped')
    if any(not isinstance(r, dict) or type(r.get('id')) is not int or type(r.get('run_attempt')) is not int or r['run_attempt'] < 1 or r.get('head_sha') != head or r.get('event') != 'pull_request' for r in runs):
        raise Refusal('workflow request identity is ambiguous')
    if len({r['id'] for r in runs}) != len(runs):
        raise Refusal('workflow request identity is duplicated')
    requests, metadata = [], []
    for run in runs:
        title = run.get('display_title')
        if title == f'Merge-ready request {head}': requests.append(run)
        elif title == f"Label metadata {run['id']}": metadata.append(run)
        else: raise Refusal('workflow request purpose is ambiguous')
    if not requests or (len(requests) > 1 and any(r['run_attempt'] != 1 for r in requests)):
        raise Refusal('workflow request is missing or rerun ordering is ambiguous')
    def created(run):
        value = run.get('created_at')
        if not isinstance(value, str) or not value.endswith('Z'):
            raise Refusal('workflow request time is missing')
        try:
            return datetime.datetime.fromisoformat(value[:-1] + '+00:00')
        except ValueError as error:
            raise Refusal('workflow request time is invalid') from error
    ordered = sorted(((created(r), r) for r in requests), key=lambda pair: pair[0])
    if len(ordered) > 1 and ordered[-1][0] == ordered[-2][0]:
        raise Refusal('latest workflow request is ambiguous')
    when, latest = ordered[-1]
    if any(r['run_attempt'] != 1 or created(r) >= when for r in metadata):
        raise Refusal('newer or rerun metadata request has unverified origin')
    if latest['id'] != run_id or latest['run_attempt'] != attempt:
        raise Refusal('requested workflow run or attempt is stale')


def require_current_head(api, pr, head):
    live = api.get(f'pulls/{pr}')
    if (not isinstance(live, dict) or type(live.get('number')) is not int or live['number'] != pr or
        live.get('state') != 'open' or not isinstance(live.get('head'), dict) or live['head'].get('sha') != head):
        raise Refusal('current PR head is missing, closed or differs from the reviewed head')


def read(api, *, pr, run_id, attempt, job_id, head, base, tree, caller_blob, leaf_blob):
    if any(not SHA.fullmatch(x or '') for x in (head, base, tree, caller_blob, leaf_blob)):
        raise Refusal('expected subject or independently reviewed workflow blob is invalid')
    current = api.get(f'actions/runs/{run_id}')
    if not isinstance(current, dict) or current.get('id') != run_id or current.get('run_attempt') != attempt:
        raise Refusal('requested attempt is stale or does not match the current run')
    if current.get('event') == 'merge_group':
        raise Refusal('merge_group checkout verification unavailable: authenticated queue subject binding is not established')
    if current.get('head_sha') != head or current.get('event') != 'pull_request':
        raise Refusal('requested attempt is stale or does not match the current run')
    require_current_head(api, pr, head)
    latest_request(api, head, run_id, attempt)
    job = api.get(f'actions/jobs/{job_id}')
    if not isinstance(job, dict) or job.get('id') != job_id or job.get('run_id') != run_id or job.get('run_attempt') != attempt or job.get('head_sha') != head:
        raise Refusal('job does not match requested run, attempt and head')
    if job.get('status') != 'completed' or job.get('conclusion') != 'success':
        raise Refusal('selected job has not completed successfully')
    if GATE.fullmatch(job.get('name') or ''):
        leaf, job_key = 'rust-gates.yml', 'rust-gates'
    elif SHARD.fullmatch(job.get('name') or ''):
        leaf, job_key = 'rust-test-shards.yml', 'rust-test-shard'
    else:
        raise Refusal('job is not a supported Rust producer')
    steps = job.get('steps')
    if not isinstance(steps, list):
        raise Refusal('job steps unavailable')
    prefixes = ('Capture checkout observation', 'Checkout source v1 ', 'Checkout parents v1 ', 'Checkout origin v1 ')
    selected = []
    for prefix in prefixes:
        matches = [s for s in steps if isinstance(s, dict) and isinstance(s.get('name'), str) and (s['name'] == prefix if prefix == prefixes[0] else s['name'].startswith(prefix))]
        if len(matches) != 1 or matches[0].get('status') != 'completed' or matches[0].get('conclusion') != 'success':
            raise Refusal('observation steps missing, ambiguous or unsuccessful')
        selected.append(matches[0])
    numbers = [s.get('number') for s in selected]
    if any(type(n) is not int for n in numbers) or numbers != list(range(numbers[0], numbers[0] + 4)):
        raise Refusal('observation step order is invalid')
    checkout = [s for s in steps if isinstance(s, dict) and s.get('number') == numbers[0] - 1]
    if (len(checkout) != 1 or not isinstance(checkout[0].get('name'), str)
        or not checkout[0]['name'].startswith('Run actions/checkout@')
        or checkout[0].get('status') != 'completed' or checkout[0].get('conclusion') != 'success'):
        raise Refusal('preceding checkout is missing or unsuccessful')
    src, parents, origin = SOURCE.fullmatch(selected[1]['name']), PARENTS.fullmatch(selected[2]['name']), ORIGIN.fullmatch(selected[3]['name'])
    if not src or not parents or not origin:
        raise Refusal('observation is malformed or unsupported')
    commit, observed_tree = src.groups()
    parent_list = [] if parents.group(1) == 'none' else parents.group(1).split(',')
    repo, workflow_sha, observed_run, observed_attempt, observed_job = origin.groups()
    if (repo, observed_run, observed_attempt, observed_job) != (REPO, str(run_id), str(attempt), job_key):
        raise Refusal('observation origin does not match the selected job')
    if parent_list != [base, head] or observed_tree != tree:
        raise Refusal('checkout differs from reviewed base, head or tree')
    detail = api.get(f'actions/runs/{run_id}/attempts/{attempt}')
    if not isinstance(detail, dict) or detail.get('id') != run_id or detail.get('run_attempt') != attempt or detail.get('head_sha') != head or detail.get('event') != 'pull_request' or detail.get('workflow_id') != WORKFLOW_ID or detail.get('path') != '.github/workflows/routed-rust.yml' or not isinstance(detail.get('repository'), dict) or detail['repository'].get('id') != REPO_ID:
        raise Refusal('workflow attempt does not match the requested repository, workflow and head')
    refs = detail.get('referenced_workflows')
    if not isinstance(refs, list):
        raise Refusal('referenced workflow metadata unavailable')
    expected_path = f'{REPO}/.github/workflows/{leaf}@{workflow_sha}'
    candidates = [r for r in refs if isinstance(r, dict) and isinstance(r.get('path'), str) and r['path'].startswith(f'{REPO}/.github/workflows/{leaf}@')]
    if len(candidates) != 1 or candidates[0].get('path') != expected_path or candidates[0].get('sha') != workflow_sha or candidates[0].get('ref') != f'refs/pull/{pr}/merge':
        raise Refusal('running reusable workflow source is missing or ambiguous')
    for name, expected in (('routed-rust.yml', caller_blob), (leaf, leaf_blob)):
        source = api.get(f'contents/.github/workflows/{name}?ref={workflow_sha}')
        if not isinstance(source, dict) or source.get('sha') != expected:
            raise Refusal('running workflow source differs from the independently reviewed blobs')
    obj = api.get(f'git/commits/{commit}')
    if not isinstance(obj, dict) or obj.get('sha') != commit or not isinstance(obj.get('tree'), dict) or obj['tree'].get('sha') != tree or not isinstance(obj.get('parents'), list) or [p.get('sha') if isinstance(p, dict) else None for p in obj['parents']] != parent_list:
        raise Refusal('GitHub commit object disagrees with the checkout observation')
    latest_request(api, head, run_id, attempt)
    require_current_head(api, pr, head)
    return {'repository': REPO, 'workflow': '.github/workflows/routed-rust.yml', 'workflow_sha': workflow_sha, 'run': run_id, 'attempt': attempt, 'job': job_id, 'job_name': job['name'], 'commit': commit, 'tree': tree, 'parents': parent_list}


class Gh:
    def get(self, path):
        try:
            result = subprocess.run(['gh', 'api', f'repos/{REPO}/{path}'], capture_output=True, text=True, timeout=45, check=False)
            if result.returncode:
                raise Refusal(f'GitHub read failed: {result.stderr.strip()}')
            return json.loads(result.stdout)
        except (OSError, subprocess.TimeoutExpired, UnicodeError, json.JSONDecodeError) as error:
            raise Refusal(f'GitHub read unavailable or invalid: {error}') from error

    def pages(self, path):
        pages = []
        for number in range(1, 11):
            page = self.get(f'{path}&page={number}')
            if not isinstance(page, dict) or not isinstance(page.get('workflow_runs'), list) or type(page.get('total_count')) is not int:
                raise Refusal('workflow request page is malformed')
            pages.append(page)
            if len(page['workflow_runs']) < 100 or sum(len(p['workflow_runs']) for p in pages) >= page['total_count']:
                return pages
        raise Refusal('workflow request inventory exceeds the page limit')


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    for flag in ('pr', 'run-id', 'attempt', 'job-id'):
        parser.add_argument('--' + flag, type=int, required=True)
    for flag in ('head', 'base', 'tree', 'caller-blob', 'leaf-blob'):
        parser.add_argument('--' + flag, required=True)
    args = parser.parse_args()
    if min(args.pr, args.run_id, args.attempt, args.job_id) <= 0:
        parser.error('PR, run, attempt and job must be positive')
    try:
        print(json.dumps(read(Gh(), **vars(args)), sort_keys=True))
    except Refusal as error:
        print(f'checkout observation refused: {error}', file=sys.stderr)
        return 1
    return 0


if __name__ == '__main__':
    sys.exit(main())
