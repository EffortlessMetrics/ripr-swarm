import json
from pathlib import Path
import sys
import tempfile
import unittest

import capture


class CaptureTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.root = Path(self.temp.name)
        self.profiles = self.root / "profiles"
        self.profiles.mkdir()
        self.output = self.root / "evidence"

    def tearDown(self):
        self.temp.cleanup()

    def runner(self, bad=None):
        def run(argv):
            if argv == ["rustc", "-vV"]:
                text = "rustc test\nhost: x86_64-unknown-linux-gnu\n"
            elif argv == ["rustc", "--print", "sysroot"]:
                text = str(self.root) + "\n"
            else:
                text = "LLVM test\n"
            result = {"exit": 0, "stdout": text, "stderr": "", "truncated": False}
            if "merge" in argv:
                self.assertEqual(argv[-1], capture.os.devnull)
                self.assertEqual(Path(argv[argv.index("-f") + 1]).read_text(), self.list_text)
                result.update(exit=1 if bad else 0, stderr=(
                    f"warning: {bad}: invalid instrumentation profile data (file header is corrupt)\n"
                    "error: no profile can be merged\n") if bad else "")
            return result
        return run

    def seed(self, content=b"profile"):
        profile = self.profiles / "workspace-123-456_0.profraw"
        profile.write_bytes(content)
        self.list_text = str(profile) + "\n"
        (self.profiles / "workspace-profraw-list").write_text(self.list_text)
        return profile

    def test_successful_replay_does_not_change_original_failure(self):
        source = self.seed()
        report = capture.capture(self.profiles, self.output, self.runner())
        self.assertEqual(report["merge_replay"]["exit"], 0)
        self.assertEqual(report["original_coverage_outcome"], "failure")
        self.assertEqual(source.read_bytes(), b"profile")
        self.assertEqual(report["captured"], [])

    def test_corrupt_and_truncated_bytes_retained_without_filtering(self):
        for data in [b"bad", b"\x81rforpl\xff" + b"\0" * 504]:
            with self.subTest(size=len(data)):
                source = self.seed(data)
                output = self.root / ("evidence-" + str(len(data)))
                report = capture.capture(self.profiles, output, self.runner(source))
                self.assertEqual(report["merge_replay"]["exit"], 1)
                saved = report["captured"][0]
                self.assertTrue(saved["complete"])
                self.assertEqual((output / saved["artifact"]).read_bytes(), data)
                self.assertEqual(source.read_bytes(), data)
                self.assertEqual(saved["pid_hint"], 123)

    def test_missing_data_is_explicit(self):
        report = capture.capture(self.root / "absent", self.output, self.runner())
        self.assertTrue(report["profiles_missing"])
        self.assertIsNone(report["merge_replay"]["exit"])

    def test_profile_prefix_and_inventory_budget(self):
        source = self.seed(b"x" * (capture.MAX_PROFILE_BYTES + 99))
        for i in range(capture.MAX_INVENTORY + 1):
            (self.profiles / f"extra-{i}.profraw").write_bytes(b"x")
        report = capture.capture(self.profiles, self.output, self.runner(source))
        self.assertTrue(report["inventory_truncated"])
        self.assertLessEqual(len(report["inventory"]), capture.MAX_INVENTORY)
        self.assertFalse(report["captured"][0]["complete"])
        self.assertEqual(report["captured"][0]["retained_bytes"], capture.MAX_PROFILE_BYTES)
        files = list(self.output.iterdir())
        self.assertLessEqual(len(files), capture.MAX_ARTIFACT_FILES)
        self.assertLessEqual(sum(f.stat().st_size for f in files), capture.MAX_ARTIFACT_BYTES)
        self.assertEqual(source.stat().st_size, capture.MAX_PROFILE_BYTES + 99)

    def test_outside_list_and_symlinks_are_not_read(self):
        outside = self.root / "credential.profraw"
        outside.write_bytes(b"not diagnostic input")
        self.list_text = str(outside) + "\n"
        (self.profiles / "workspace-profraw-list").write_text(self.list_text)
        report = capture.capture(self.profiles, self.output, self.runner(outside))
        self.assertFalse(report["profile_list_replay_safe"])
        self.assertEqual(report["captured"], [])
        link = self.root / "link"
        link.symlink_to(self.profiles)
        with self.assertRaises(ValueError):
            capture.capture(link, self.root / "second", self.runner())

    def test_tool_output_is_bounded_and_drained(self):
        result = capture.bounded_tool([sys.executable, "-c",
                                      "import sys; sys.stdout.write('x'*100000); sys.stderr.write('y'*100000)"])
        self.assertEqual(result["exit"], 0)
        self.assertTrue(result["truncated"])
        self.assertEqual(len(result["stdout"]), capture.MAX_TOOL_BYTES)
        self.assertEqual(len(result["stderr"]), capture.MAX_TOOL_BYTES)

    def test_tool_deadline_reaps_the_owned_process(self):
        result = capture.bounded_tool([sys.executable, "-c", "import time; time.sleep(60)"], timeout=0.05)
        self.assertTrue(result["timed_out"])
        self.assertNotEqual(result["exit"], 0)

    def test_failed_profile_count_budget_and_explicit_list(self):
        profiles = []
        for index in range(capture.MAX_PROFILES + 2):
            path = self.profiles / f"workspace-{index}-44_0.profraw"
            path.write_bytes(b"bad")
            profiles.append(path)
        listing = self.profiles / "workspace-profraw-list"
        self.list_text = "".join(str(p) + "\n" for p in profiles)
        listing.write_text(self.list_text)
        def run(argv):
            result = self.runner()(argv)
            if "merge" in argv:
                result.update(exit=1, stderr="".join(
                    f"warning: {p}: invalid instrumentation profile data (file header is corrupt)\n"
                    for p in profiles))
            return result
        report = capture.capture(self.profiles, self.output, run, profile_list=listing)
        self.assertEqual(len(report["captured"]), capture.MAX_PROFILES)
        self.assertTrue(report["failed_profile_capture_truncated"])
        self.assertTrue(all(p.read_bytes() == b"bad" for p in profiles))


if __name__ == "__main__":
    unittest.main()
