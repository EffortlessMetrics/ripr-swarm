"""Real OS process controls using Python children, not Cargo or an agent."""
import os
from pathlib import Path
import subprocess
import sys
import tempfile
import time
import unittest

from scoring import _run_process, execution_receipt, sha256


def _proc_state_after_wait(child_pid, deadline_seconds):
    """Poll /proc until the descendant is dead (Z/X) or the deadline passes.

    A single read right after the timeout kill races the reap on a loaded
    runner, so wait for the terminal state; return the last observed state
    (None when /proc no longer lists the pid).
    """
    proc = Path(f"/proc/{child_pid}/stat")
    deadline = time.monotonic() + deadline_seconds
    while True:
        try:
            state = proc.read_text().split()[2]
        except (FileNotFoundError, ProcessLookupError):
            return None
        if state in ("Z", "X") or time.monotonic() >= deadline:
            return state
        time.sleep(0.05)


class ProcessControlTests(unittest.TestCase):
    def test_native_child_exit_and_raw_output_are_retained(self):
        with tempfile.TemporaryDirectory() as directory:
            result = _run_process([sys.executable, "-c", "import sys;sys.stdout.buffer.write(b'\\xff');sys.exit(7)"],
                                  directory, dict(os.environ), 5)
        self.assertEqual(result.returncode, 7)
        self.assertEqual(result.stdout, b"\xff")
        receipt = execution_receipt(result)
        self.assertEqual(receipt["stdout_bytes"], 1)
        self.assertEqual(receipt["stdout_sha256"], sha256(b"\xff"))
        self.assertEqual(receipt["status"], "process_error")

    def test_missing_program_is_a_launch_failure_not_compile_failure(self):
        with tempfile.TemporaryDirectory() as directory:
            result = _run_process([str(Path(directory) / "not-an-executable")], directory, dict(os.environ), 5)
        self.assertTrue(result.cohort_launch_error)
        result.cohort_phase = "compile"
        self.assertEqual(execution_receipt(result)["status"], "process_error")

    def test_timeout_terminates_the_owned_child_tree(self):
        code = ("import subprocess,sys,time;"
                "p=subprocess.Popen([sys.executable,'-c','import time;time.sleep(30)']);"
                "print(p.pid,flush=True);time.sleep(30)")
        with tempfile.TemporaryDirectory() as directory:
            result = _run_process([sys.executable, "-c", code], directory, dict(os.environ), 2)
        self.assertTrue(result.cohort_timed_out)
        self.assertIsNone(result.cohort_cleanup_error)
        self.assertEqual(execution_receipt(result)["status"], "timeout")
        child_pid = int(result.stdout.strip())
        if os.name == "posix":
            if Path(f"/proc/{child_pid}/stat").exists():
                state = _proc_state_after_wait(child_pid, 10)
                if state is not None:
                    self.assertIn(state, ("Z", "X"), "owned descendant is still executing")
            else:
                check = subprocess.run(["ps", "-o", "stat=", "-p", str(child_pid)], capture_output=True, text=True, timeout=5)
                self.assertTrue(not check.stdout.strip() or check.stdout.strip().startswith("Z"), check.stdout)
        else:
            check = subprocess.run(["tasklist", "/FI", f"PID eq {child_pid}", "/FO", "CSV", "/NH"],
                                   capture_output=True, text=True, timeout=5)
            self.assertNotIn(f'"{child_pid}"', check.stdout, "owned descendant remains in tasklist")

    @unittest.skipUnless(os.name == "posix" and Path("/proc/self/stat").exists(), "needs /proc")
    def test_state_poll_still_reports_a_live_descendant(self):
        # Negative control for the bounded poll: a descendant that is not
        # terminated must still read as executing after the deadline.
        child = subprocess.Popen([sys.executable, "-c", "import time;time.sleep(30)"])
        try:
            state = _proc_state_after_wait(child.pid, 0.3)
            self.assertNotIn(state, ("Z", "X", None))
        finally:
            child.kill()
            child.wait(timeout=5)
