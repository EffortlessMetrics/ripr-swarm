"""Real OS process controls using Python children, not Cargo or an agent."""
import os
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest
from unittest import mock

import scoring
from scoring import _run_process, execution_receipt, sha256


class ProcessControlTests(unittest.TestCase):
    def test_reaped_parent_does_not_prove_owned_group_cleanup(self):
        child = mock.MagicMock(pid=123)
        child.communicate.side_effect = [subprocess.TimeoutExpired("fixture", 1), (b"usage", b"stderr")]
        child.poll.return_value = -9
        process = mock.MagicMock()
        process.__enter__.return_value = child
        with mock.patch.object(scoring.os, "name", "posix"), \
             mock.patch.object(scoring.os, "killpg", create=True), \
             mock.patch.object(scoring.signal, "SIGKILL", 9, create=True), \
             mock.patch.object(scoring.subprocess, "Popen", return_value=process), \
             mock.patch.object(scoring.subprocess, "run", return_value=subprocess.CompletedProcess([], 0, "123 S\n", "")), \
             mock.patch.object(scoring.time, "monotonic", side_effect=[0, 0, 5]):
            result = _run_process(["fixture"], ".", {}, 1)
        self.assertEqual(result.stdout, b"usage")
        self.assertEqual(result.stderr, b"stderr")
        self.assertEqual(execution_receipt(result)["status"], "cleanup_error")
        self.assertIn("still executing", result.cohort_cleanup_error)

    def test_group_cleanup_waits_for_quiescence_and_refuses_uncertain_snapshots(self):
        active = subprocess.CompletedProcess([], 0, "123 S\n999 R\n", "")
        stopped = subprocess.CompletedProcess([], 0, "123 Z\n123 X\n999 R\n", "")
        with mock.patch.object(scoring.subprocess, "run", side_effect=[active, stopped]) as probe, \
             mock.patch.object(scoring.time, "sleep"):
            self.assertIsNone(scoring._group_cleanup_error(123))
        self.assertEqual(probe.call_count, 2)
        for output in ("", "unreadable", "123", "² Z\n", "١٢٣ Z\n", "9" * 5000 + " Z\n"):
            with self.subTest(output=output), mock.patch.object(scoring.subprocess, "run",
                    return_value=subprocess.CompletedProcess([], 0, output, "")):
                self.assertIn("snapshot", scoring._group_cleanup_error(123))
        for error in (OSError("unavailable"), subprocess.CalledProcessError(1, "ps"),
                      subprocess.TimeoutExpired("ps", 5), UnicodeDecodeError("utf-8", b"\xff", 0, 1, "bad")):
            with self.subTest(error=error), mock.patch.object(scoring.subprocess, "run", side_effect=error):
                self.assertIn("inspect", scoring._group_cleanup_error(123))

    def test_group_cleanup_requires_complete_single_thread_stop_state(self):
        for state in ("Z", "X", "Z<", "ZN", "ZL", "Zs", "Z+", "ZNLs+", "X<Ls+"):
            snapshot = subprocess.CompletedProcess([], 0, f"123 {state}\n999 R\n", "")
            with self.subTest(state=state), mock.patch.object(scoring.subprocess, "run", return_value=snapshot):
                self.assertIsNone(scoring._group_cleanup_error(123, timeout=0))
        # A zombie thread-group leader may still have executing sibling threads
        # (procps STAT flag l); neither that nor malformed tokens prove cleanup.
        for state in ("ZombieGarbage", "X?", "Zl", "Zsl+", "Xl", "Z++", "ZNN", "Z<N", "ZZ"):
            snapshot = subprocess.CompletedProcess([], 0, f"123 {state}\n999 R\n", "")
            with self.subTest(state=state), mock.patch.object(scoring.subprocess, "run", return_value=snapshot):
                self.assertIsNotNone(scoring._group_cleanup_error(123, timeout=0))

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
