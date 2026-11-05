"""Real OS process controls using Python children, not Cargo or an agent."""
import os
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest

from scoring import _run_process, execution_receipt, sha256


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
            proc = Path(f"/proc/{child_pid}/stat")
            if proc.exists():
                self.assertIn(proc.read_text().split()[2], ("Z", "X"), "owned descendant is still executing")
            else:
                check = subprocess.run(["ps", "-o", "stat=", "-p", str(child_pid)], capture_output=True, text=True, timeout=5)
                self.assertTrue(not check.stdout.strip() or check.stdout.strip().startswith("Z"), check.stdout)
        else:
            check = subprocess.run(["tasklist", "/FI", f"PID eq {child_pid}", "/FO", "CSV", "/NH"],
                                   capture_output=True, text=True, timeout=5)
            self.assertNotIn(f'"{child_pid}"', check.stdout, "owned descendant remains in tasklist")
