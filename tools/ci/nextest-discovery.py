#!/usr/bin/env python3
"""Bound Linux Nextest discovery; publish metadata only and complete canonical lists."""
import json
import os
from pathlib import Path
import re
import selectors
import signal
import subprocess
import sys
import tempfile
import time

cancelled = 0

class Cancelled(Exception):
    pass

def record_cancel(sig, _frame):
    global cancelled
    if not cancelled:
        cancelled = sig


def cancel(sig, frame):
    record_cancel(sig, frame)
    raise Cancelled()


def stop_group(proc):
    # Repeated TERM/INT must not interrupt teardown before the direct child is reaped.
    previous = {sig: signal.signal(sig, record_cancel)
                for sig in (signal.SIGTERM, signal.SIGINT)}
    try:
        # Kill surviving descendants even if the direct child has already exited.
        for sig in (signal.SIGTERM, signal.SIGKILL):
            try:
                os.killpg(proc.pid, sig)
            except ProcessLookupError:
                pass
            if sig == signal.SIGTERM:
                time.sleep(0.1)
        proc.wait()
    finally:
        for sig, handler in previous.items():
            signal.signal(sig, handler)


def main():
    phase, name, seconds, *command = sys.argv[1:]
    if not re.fullmatch(r'(partition-[1-9][0-9]*|full)', phase) or not command:
        return 64
    budget = float(seconds)
    if not 0 < budget <= 2400:
        return 64
    canonical = Path(name)
    canonical.parent.mkdir(parents=True, exist_ok=True)
    status = canonical.parent / f'discovery-{phase}.status'
    diagnostic = canonical.parent / f'discovery-{phase}.metadata.json'
    # Do not reuse an earlier successful phase on any new invocation.
    for path in (canonical, status, diagnostic):
        path.unlink(missing_ok=True)
    signal.signal(signal.SIGTERM, cancel)
    signal.signal(signal.SIGINT, cancel)
    start = time.monotonic()
    proc = None
    state, code = 'failed', 70
    out = bytearray()
    stderr_bytes = 0
    child_exit = None
    try:
        with tempfile.TemporaryDirectory(prefix=f'.discovery-{phase}-', dir=canonical.parent) as stage:
            stage = Path(stage)
            # A raising handler between Popen's return and STORE_FAST would lose
            # the only owned handle. Record cancellation until assignment completes.
            signal.signal(signal.SIGTERM, record_cancel)
            signal.signal(signal.SIGINT, record_cancel)
            proc = subprocess.Popen(command, stdout=subprocess.PIPE, stderr=subprocess.PIPE,
                                    start_new_session=True)
            if cancelled:
                raise Cancelled()
            signal.signal(signal.SIGTERM, cancel)
            signal.signal(signal.SIGINT, cancel)
            if cancelled:
                raise Cancelled()
            sel = selectors.DefaultSelector()
            sel.register(proc.stdout, selectors.EVENT_READ, 'stdout')
            sel.register(proc.stderr, selectors.EVENT_READ, 'stderr')
            while sel.get_map() or proc.poll() is None:
                if cancelled:
                    state, code = 'cancelled', 128 + cancelled
                    break
                if time.monotonic() - start >= budget:
                    state, code = 'timeout', 124
                    break
                for key, _ in sel.select(0.05):
                    chunk = os.read(key.fileobj.fileno(), 65536)
                    if not chunk:
                        sel.unregister(key.fileobj)
                        continue
                    if key.data == 'stdout':
                        if len(out) + len(chunk) > 64 * 1024 * 1024:
                            state, code = 'output-limit', 65
                            break
                        out.extend(chunk)
                    else:
                        stderr_bytes += len(chunk)  # Never retain child diagnostics.
                if state == 'output-limit':
                    break
            if state in ('cancelled', 'timeout', 'output-limit'):
                stop_group(proc)
            else:
                child_exit = proc.wait()
                code = child_exit if child_exit >= 0 else 128 - child_exit
                state = 'child-failed' if code else 'complete'
                try:
                    os.killpg(proc.pid, 0)
                except ProcessLookupError:
                    pass
                else:
                    stop_group(proc)
                    state, code = 'leftover-processes', 71
                if not code and not out.strip():
                    state, code = 'empty', 65
            child_exit = proc.returncode
            sel.close()
            proc.stdout.close()
            proc.stderr.close()
            if code == 0:
                lines = bytes(out).split(b'\n')
                if lines[-1] == b'':
                    lines.pop()
                (stage / 'canonical').write_bytes(b'\n'.join(sorted(lines)) + b'\n')
                if time.monotonic() - start >= budget:
                    state, code = 'timeout', 124
            # Metadata only: no raw/decoded child stderr or command arguments.
            diag = json.dumps({'phase': phase, 'stderr_bytes': stderr_bytes,
                               'stderr_bucket': 'present' if stderr_bytes else 'empty'}) + '\n'
            (stage / 'stderr').write_text(diag, encoding='ascii')
            os.replace(stage / 'stderr', diagnostic)
            if cancelled:
                state, code = 'cancelled', 128 + cancelled
            receipt = {'phase': phase, 'state': state, 'exit': code, 'child_exit': child_exit,
                       'elapsed_seconds': round(time.monotonic() - start, 3)}
            (stage / 'status').write_text(json.dumps(receipt) + '\n')
            os.replace(stage / 'status', status)
            if code == 0:
                if cancelled:
                    raise InterruptedError('cancelled before publication')
                os.replace(stage / 'canonical', canonical)
            # Consumer-ready assertion checks actual published files, not code alone.
            if code == 0:
                saved = json.loads(status.read_text())
                if not (canonical.is_file() and canonical.stat().st_size > 0
                        and saved['state'] == 'complete' and saved['exit'] == 0
                        and diagnostic.is_file()):
                    raise OSError('consumer evidence incomplete')
    except Cancelled:
        signal.signal(signal.SIGTERM, signal.SIG_IGN)
        signal.signal(signal.SIGINT, signal.SIG_IGN)
        if proc is not None:
            stop_group(proc)
        canonical.unlink(missing_ok=True)
        state, code = 'cancelled', 128 + cancelled
        try:
            status.write_text(json.dumps({'phase': phase, 'state': state, 'exit': code,
                'elapsed_seconds': round(time.monotonic()-start, 3)}) + '\n')
        except OSError:
            try:
                status.unlink(missing_ok=True)
            except OSError:
                pass
    except (OSError, ValueError) as exc:
        if proc is not None:
            stop_group(proc)
        # Failed evidence write or publication cannot leave a qualified list/status.
        canonical.unlink(missing_ok=True)
        try:
            status.unlink(missing_ok=True)
        except OSError:
            pass  # An obstructing directory is not a valid success receipt.
        print(f'[discovery:{phase}] evidence/setup failure: {type(exc).__name__}', file=sys.stderr)
        return 128 + cancelled if cancelled else 74
    return code


if __name__ == '__main__':
    sys.exit(main())
