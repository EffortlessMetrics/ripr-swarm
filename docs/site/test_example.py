"""Check the page's local links and execute its boundary-test example."""

from html.parser import HTMLParser
from pathlib import Path
import os
import shlex
import shutil
import subprocess
import tempfile
import unittest
from urllib.parse import urlsplit


SITE = Path(__file__).resolve().parent
ROOT = SITE.parent.parent
FIXTURE = ROOT / "fixtures" / "boundary_gap"


class Page(HTMLParser):
    def __init__(self, source):
        super().__init__(convert_charrefs=True)
        self.ids = []
        self.links = []
        self.assets = []
        self.blocks = {}
        self.code = None
        self.feed(source)

    def handle_starttag(self, tag, attributes):
        attributes = dict(attributes)
        if "id" in attributes:
            self.ids.append(attributes["id"])
        if tag == "a" and "href" in attributes:
            self.links.append(attributes["href"])
        if tag in ("img", "link"):
            self.assets.append(attributes.get("src", attributes.get("href", "")))
        if tag == "code" and "id" in attributes:
            self.code = attributes["id"]
            self.blocks[self.code] = ""

    def handle_data(self, data):
        if self.code is not None:
            self.blocks[self.code] += data

    def handle_endtag(self, tag):
        if tag == "code":
            self.code = None


class ReleaseWebsite(unittest.TestCase):
    def setUp(self):
        self.page = Page((SITE / "index.html").read_text())

    def block(self, name):
        self.assertIn(name, self.page.blocks, f"missing documented example step: {name}")
        code = self.page.blocks[name].strip()
        self.assertTrue(code, f"empty example step: {name}")
        return code

    def test_local_links_and_assets(self):
        self.assertEqual(len(self.page.ids), len(set(self.page.ids)), "duplicate HTML id")
        for href in self.page.links:
            if href.startswith("#"):
                self.assertIn(href[1:], self.page.ids, f"broken local link: {href}")
        for asset in self.page.assets:
            url = urlsplit(asset)
            if not url.scheme and not url.netloc:
                self.assertTrue((SITE / url.path).is_file(), f"missing asset: {asset}")

    def test_development_fixture_command_and_output(self):
        command = self.block("fixture-command")
        self.assertIn(command, (FIXTURE / "SPEC.md").read_text().splitlines())
        excerpt = self.block("fixture-output").splitlines()
        self.assertGreaterEqual(len(excerpt), 3, "the example must show actual evidence")
        expected = {line.strip() for line in (FIXTURE / "expected/human.txt").read_text().splitlines()}
        self.assertTrue(all(line.strip() in expected for line in excerpt))

    def test_equality_assertion_rejects_the_old_behavior(self):
        test = self.block("boundary-test")
        command = shlex.split(self.block("boundary-test-command"))
        self.assertEqual(command, ["cargo", "test", "equality_gets_discount"])
        original_tests = (FIXTURE / "input/tests/pricing.rs").read_text()
        for assertion in self.block("existing-tests").splitlines():
            self.assertIn(assertion.strip(), original_tests)

        with tempfile.TemporaryDirectory(prefix="ripr-site-example-") as directory:
            root = Path(directory)
            shutil.copytree(FIXTURE / "input/src", root / "src")
            (root / "tests").mkdir()
            (root / "Cargo.toml").write_text(
                (FIXTURE / "input/Cargo.toml").read_text() + "\n[workspace]\n"
            )
            (root / "tests/pricing.rs").write_text(original_tests + "\n" + test + "\n")
            env = dict(os.environ, CARGO_TARGET_DIR=str(root / "target"), CARGO_NET_OFFLINE="true")

            def run(arguments):
                return subprocess.run(
                    arguments, cwd=root, env=env, capture_output=True, text=True, timeout=60
                )

            new = run(command)
            self.assertEqual(new.returncode, 0, new.stdout + new.stderr)
            self.assertIn("running 1 test", new.stdout)
            self.assertRegex(new.stdout, r"test equality_gets_discount \.\.\. ok")

            source = (root / "src/lib.rs").read_text()
            self.assertEqual(source.count("amount >= discount_threshold"), 1)
            (root / "src/lib.rs").write_text(source.replace("amount >=", "amount >"))
            for name in ("below_threshold_has_no_discount", "far_above_threshold_discounts"):
                old_existing = run(["cargo", "test", name])
                self.assertEqual(old_existing.returncode, 0, old_existing.stdout + old_existing.stderr)
                self.assertIn("running 1 test", old_existing.stdout)

            old = run(command)
            self.assertNotEqual(old.returncode, 0, "the equality test must reject the old > condition")
            self.assertIn("equality_gets_discount ... FAILED", old.stdout)
            self.assertRegex(old.stdout, r"left:\s+100")
            self.assertRegex(old.stdout, r"right:\s+90")


if __name__ == "__main__":
    unittest.main()
