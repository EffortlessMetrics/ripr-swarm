"""Replay captured fixture sources; historical executed drivers remain unchanged.

Use an absent --out directory. This successor accepts receiver-local executables.
It checks captured source hashes before compiling and validates real test output.
"""
import argparse
import atexit
import hashlib
import json
from pathlib import Path
import shutil
import subprocess
import sys


def sha(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def run(args, cwd, stem):
    command = [str(x) for x in args]
    result = subprocess.run(command, cwd=cwd, capture_output=True, text=True, timeout=120)
    stem.with_suffix('.stdout').write_text(result.stdout)
    stem.with_suffix('.stderr').write_text(result.stderr)
    return {'command': command, 'cwd': str(cwd), 'exit': result.returncode}


def main():
    if not __debug__:
        raise RuntimeError('Replay requires Python validation enabled; do not use -O or PYTHONOPTIMIZE')
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--receipts', type=Path, required=True, help='extracted receipts root')
    parser.add_argument('--analyzer', type=Path, required=True)
    parser.add_argument('--rustc', type=Path, required=True)
    parser.add_argument('--expected-revision', required=True, help='full committed build identity reported by ripr --version')
    parser.add_argument('--out', type=Path, required=True)
    parser.add_argument('--case', action='append', default=[])
    parser.add_argument('--all', action='store_true')
    args = parser.parse_args()
    if args.all == bool(args.case):
        parser.error('select --all or one or more --case names')
    args.analyzer = args.analyzer.resolve(strict=True)
    args.rustc = args.rustc.resolve(strict=True)
    args.out = args.out.resolve()
    args.out.mkdir(parents=True, exist_ok=False)
    captured = args.receipts.resolve() / 'repair/final-d2d9'
    data = json.loads((captured / 'results.json').read_text())
    wanted = set(args.case)
    rows = [r for r in data['results'] if args.all or r['case'] in wanted]
    assert rows and (args.all or {r['case'] for r in rows} == wanted), 'unknown/missing case'
    result = {'historical_receipt_identity': data['identity'], 'complete': False, 'outcome': 'incomplete',
              'analyzer': str(args.analyzer), 'analyzer_sha256': sha(args.analyzer),
              'analyzer_version': subprocess.check_output([str(args.analyzer), '--version'], text=True).strip(),
              'rustc': str(args.rustc),
              'rustc_version': subprocess.check_output([str(args.rustc), '-vV'], text=True).strip(),
              'scope': 'fresh receiver-local replay; no relabeling of historical execution',
              'results': []}
    def persist():
        (args.out / 'results.json').write_text(json.dumps(result, indent=2) + '\n')
    original_hook = sys.excepthook
    def report_error(kind, error, traceback):
        result['outcome'] = 'failed'
        result['error'] = kind.__name__ + ': ' + str(error)
        persist()
        original_hook(kind, error, traceback)
    sys.excepthook = report_error
    atexit.register(persist)
    persist()
    assert len(args.expected_revision) == 40 and args.expected_revision in result['analyzer_version'], 'unexpected analyzer build revision'
    for row in rows:
        name = row['case']
        record = {'case': name, 'state': 'in_progress', 'native': {}}
        result['results'].append(record)
        persist()
        source = captured / 'cases' / name
        case = args.out / name
        (case / 'src').mkdir(parents=True)
        for variant, relative in [('original', 'src/lib.rs'), ('mutant', 'mutant.rs')]:
            assert sha(source / relative) == row['runtime'][variant]['source_sha256'], (name, variant)
            shutil.copyfile(source / relative, case / relative)
        for relative in ['Cargo.toml', 'diff.patch']:
            shutil.copyfile(source / relative, case / relative)
        analyzed = run([args.analyzer, 'check', '--root', case, '--diff', case / 'diff.patch',
                        '--mode', 'ready', '--format', 'json'], case, case / 'analysis')
        record['analysis'] = analyzed
        persist()
        assert analyzed['exit'] == 0, (name, 'analyzer setup')
        findings = json.loads((case / 'analysis.stdout').read_text())['findings']
        selected = [f for f in findings if f['probe']['line'] == row['line']
                    and f['probe']['family'] == 'return_value']
        assert len(selected) == 1, (name, 'missing/ambiguous finding')
        actual = selected[0].get('classification', selected[0].get('class'))
        record['class'] = actual
        persist()
        assert (actual == 'exposed') == row['expected_credit'], (name, actual)
        native = {}
        for variant, relative in [('original', 'src/lib.rs'), ('mutant', 'mutant.rs')]:
            executable = case / (variant + '-test')
            compiled = run([args.rustc, '--edition=2024', '--test', case / relative,
                            '-C', 'debuginfo=0', '-o', executable], case, case / (variant + '-compile'))
            record['native'][variant] = {'source_sha256': sha(case / relative), 'compile': compiled}
            persist()
            assert compiled['exit'] == 0, (name, variant, 'compile')
            executed = run([executable, '--test-threads=1'], case, case / (variant + '-run'))
            record['native'][variant].update({'binary_sha256': sha(executable), 'run': executed})
            persist()
            output = (case / (variant + '-run.stdout')).read_text()
            assert 'running 1 test' in output and '0 ignored' in output, (name, variant, 'unobserved test')
            expected = row['expected_native'][variant]
            assert expected is None or executed['exit'] == expected, (name, variant, executed['exit'], expected)
            assert executed['exit'] in [0, 101], (name, variant, 'runtime instrument failure')
            native[variant] = {'source_sha256': sha(case / relative), 'binary_sha256': sha(executable),
                               'compile': compiled, 'run': executed, 'expected_exit': expected}
        record.update({'state': 'passed', 'native': native})
        persist()
        print(name, actual, native['original']['run']['exit'], native['mutant']['run']['exit'], flush=True)
    result.update({'complete': True, 'outcome': 'passed'})
    persist()


if __name__ == '__main__':
    main()
