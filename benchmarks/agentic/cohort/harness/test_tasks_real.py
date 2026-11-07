"""Source-only controls for removing upstream tests before mutation generation."""
from pathlib import Path
import tempfile
import unittest
from unittest import mock

import tasks_real


class RealTaskSourceTests(unittest.TestCase):
    def test_standalone_test_with_stacked_attributes_and_raw_strings_is_removed(self):
        before = 'pub fn before() -> bool { true }\n'
        after = 'pub fn after() -> bool { false }\n'
        test = r'''#[allow(unused)]
#[test]
#[cfg_attr(not(feature = "std"), allow(unreachable_code, unused_mut))]
fn retained_upstream_test() {
    let cases = r###"
        <{one, two}> => <'{one, two}'>
        unmatched } ] ) and #[test] text
    "###;
    /* } /* { nested */ ] */
    let closing = '}';
    let escaped = '\'';
    assert!(cases.len() == 2);
}
'''
        self.assertEqual(tasks_real.strip_tests(before + test + after), before + "\n" + after)

    def test_cfg_test_imports_data_and_modules_are_removed_as_complete_items(self):
        production = 'pub fn retained() -> u8 { 7 }\n'
        cases = [
            '#[cfg(test)]\nuse alloc::{vec, borrow::ToOwned};\n',
            '#[cfg(test)]\nstatic CASES: &[(&str, u8)] = &[("; }", 1)];\n',
            '#[cfg(test)]\nconst DATA: &[u8] = b"\\xa1";\n',
            '#[cfg(test)]\nmod tests { #[test] fn nested() { assert_eq!("}", "}"); } }\n',
            '#[cfg(test)]\nmod external_tests;\n',
        ]
        for removed in cases:
            with self.subTest(item=removed):
                self.assertEqual(tasks_real.strip_tests(removed + production).lstrip(), production)

    def test_production_comments_literals_attributes_and_lifetimes_stay_byte_exact(self):
        source = r'''// #[test] fn comment() {}
/* #[cfg(test)] mod comment { } */
#[cfg(feature = "test")]
pub fn text<'a>(input: &'a str) -> &'a str {
    let normal = "#[test] fn literal() { }";
    let raw = br#"#[cfg(test)] static DATA: [u8; 1] = [1];"#;
    let ch = '\u{007d}';
    input
}
'''
        self.assertEqual(tasks_real.strip_tests(source), source)

    def test_unknown_test_attributes_and_incomplete_items_fail_closed(self):
        cases = [
            '#[cfg(any(test, feature="extra"))]\nfn maybe_test() {}',
            '#![cfg(test)]\nfn all_test() {}',
            '#[cfg(test)]\nstruct UnknownTestItem {}',
            '#[test]\nfn unfinished() {',
            '#[test]\nfn unfinished() { let s = r#"unterminated; }',
        ]
        for source in cases:
            with self.subTest(source=source), self.assertRaises(ValueError):
                tasks_real.strip_tests(source)

    def test_real_entrypoint_strips_extra_rust_files_and_isolates_unavailable_tasks(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            (root / "src").mkdir()
            (root / "src/lib.rs").write_text('pub mod bytes;\n#[test]\nfn test_lib() {}\n')
            (root / "src/bytes.rs").write_text(
                'pub fn value() -> u8 { 3 }\n#[cfg(test)]\nconst TEST: u8 = 1;\n#[test]\nfn test_extra() {}\n')
            with mock.patch.object(tasks_real.glob, "glob", return_value=[directory]), \
                 mock.patch.dict(tasks_real.REAL, {}, clear=True), \
                 mock.patch.dict(tasks_real.UNAVAILABLE, {}, clear=True):
                tasks_real.real("good", "fixture-1", "manifest", "weak tests", extra=("bytes.rs",))
                task = tasks_real.REAL["good"]
                self.assertEqual(task["new_src"].strip(), "pub mod bytes;")
                self.assertEqual(task["extra_files"]["src/bytes.rs"].strip(), "pub fn value() -> u8 { 3 }")
                (root / "src/lib.rs").write_text('#[cfg(test)]\nstruct Unsupported {}\n')
                tasks_real.real("bad", "fixture-2", "manifest", "weak tests")
                self.assertNotIn("bad", tasks_real.REAL)
                self.assertIn("unsupported", tasks_real.UNAVAILABLE["bad"])
                self.assertIs(tasks_real.REAL["good"], task)
