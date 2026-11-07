"""One native-run acceptance deadline. This is not an Actions cancellation service."""
from datetime import datetime, timezone
import hashlib
import json
import os
from pathlib import Path
import re
import time

SECONDS = 20 * 60


def require(condition, detail):
    if not condition:
        raise ValueError(detail)


def unique_object(pairs):
    result = {}
    for key, value in pairs:
        require(key not in result, 'Duplicate budget field: ' + key)
        result[key] = value
    return result


def decode(text):
    value = json.loads(text, object_pairs_hook=unique_object)
    require(isinstance(value, dict), 'Budget must be an object')
    return value


def canonical(value):
    return json.dumps(value, sort_keys=True, separators=(',', ':'))


def context(env):
    require(env.get('GITHUB_EVENT_NAME') == 'merge_group', 'Budget requires native merge_group')
    repository = env.get('GITHUB_REPOSITORY', '')
    require(repository == 'EffortlessMetrics/ripr-swarm', 'Foreign budget repository')
    for name in ['GITHUB_RUN_ID', 'GITHUB_RUN_ATTEMPT']:
        require(re.fullmatch('[1-9][0-9]*', env.get(name, '')), 'Malformed ' + name)
    for name in ['GITHUB_SHA', 'GITHUB_WORKFLOW_SHA']:
        require(re.fullmatch('[0-9a-f]{40}', env.get(name, '')), 'Malformed ' + name)
    require(env.get('BUDGET_SUBJECT') == env['GITHUB_SHA'], 'Rebound budget subject')
    return dict(repository=repository, run_id=env['GITHUB_RUN_ID'],
                run_attempt=env['GITHUB_RUN_ATTEMPT'], subject=env['GITHUB_SHA'],
                generation=env['GITHUB_WORKFLOW_SHA'])


def issue(metadata, env, now):
    expected = context(env)
    for key, field in [('run_id', 'id'), ('run_attempt', 'run_attempt')]:
        require(type(metadata.get(field)) is int and str(metadata[field]) == expected[key],
                'Wrong native run ' + field)
    require(metadata.get('repository', {}).get('full_name') == expected['repository'],
            'Wrong native run repository')
    require(metadata.get('event') == 'merge_group', 'Wrong native run event')
    require(metadata.get('head_sha') == expected['subject'], 'Wrong native run checkout')
    started = metadata.get('run_started_at', '')
    require(isinstance(started, str) and re.fullmatch(r'[0-9]{4}(-[0-9]{2}){2}T[0-9]{2}(:[0-9]{2}){2}Z', started),
            'Missing canonical native run start')
    epoch = int(datetime.strptime(started, '%Y-%m-%dT%H:%M:%SZ').replace(tzinfo=timezone.utc).timestamp())
    require(epoch <= now, 'Native run start is in the future')
    return canonical(dict(expected, version=1, started_epoch=epoch, deadline_epoch=epoch + SECONDS))


def verify(receipt, env, now, origin):
    expected = context(env)
    require(receipt == origin, 'Budget differs from the immutable native-run artifact')
    value = decode(receipt)
    require(set(value) == set(expected) | {'version', 'started_epoch', 'deadline_epoch'},
            'Missing or unexpected budget fields')
    require(canonical(value) == receipt, 'Noncanonical budget receipt')
    require(type(value['version']) is int and value['version'] == 1, 'Unknown budget version')
    for key, wanted in expected.items():
        require(value[key] == wanted, 'Wrong budget ' + key)
    for key in ['started_epoch', 'deadline_epoch']:
        require(type(value[key]) is int and value[key] > 0, 'Malformed budget ' + key)
    require(value['deadline_epoch'] == value['started_epoch'] + SECONDS, 'Budget was renewed or extended')
    require(value['started_epoch'] <= now < value['deadline_epoch'], 'Qualification deadline expired or not started')
    return value['deadline_epoch'] - now


def main():
    env = os.environ
    now = int(time.time())
    receipt = env.get('BUDGET_RECEIPT', '')
    mode = env.get('BUDGET_ISSUE', '')
    require(mode in ['true', 'false'], 'Unknown budget issue mode')
    if mode == 'true':
        require(not receipt, 'Issuer cannot replace a supplied budget')
        metadata = decode(Path(env['BUDGET_METADATA']).read_text(encoding='utf-8'))
        receipt = issue(metadata, env, now)
        origin = receipt
    else:
        origin = Path(__file__).with_name('receipt.json').read_text(encoding='utf-8')
    remaining = verify(receipt, env, now, origin)
    digest = ''
    descriptor_digest = ''
    if mode == 'true':
        source = Path(__file__).read_bytes()
        staging = Path(env['RUNNER_TEMP']) / 'qualification-budget'
        staging.mkdir(exist_ok=False)
        (staging / 'budget.py').write_bytes(source)
        descriptor = (Path(__file__).parent.parent / 'consume-qualification-budget' / 'action.yml').read_bytes()
        (staging / 'action.yml').write_bytes(descriptor)
        (staging / 'receipt.json').write_text(receipt, encoding='utf-8')
        digest = hashlib.sha256(source).hexdigest()
        descriptor_digest = hashlib.sha256(descriptor).hexdigest()
    with Path(env['GITHUB_OUTPUT']).open('a', encoding='utf-8') as output:
        output.write('receipt=' + receipt + '\nremaining_seconds=' + str(remaining) + '\n')
        if digest:
            output.write('verifier_sha256=' + digest + '\n')
            output.write('descriptor_sha256=' + descriptor_digest + '\n')


if __name__ == '__main__':
    main()
