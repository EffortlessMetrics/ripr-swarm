"""Exercise the actual inline Actions consumer with synthetic files; never build RIPR."""
import ast
import errno
import hashlib
import json
import os
from pathlib import Path
import shutil
import sys
import tempfile
import unittest
from unittest.mock import patch

WORKFLOW = Path(__file__).resolve().parents[3] / '.github/workflows/windows-advisory.yml'


def consumer():
    text = WORKFLOW.read_text()
    prefix, remainder = text.split("$consumer = @'\n", 1)
    raw = remainder.split("          '@", 1)[0]
    tree = ast.parse('\n'.join(line[10:] for line in raw.splitlines()))
    ast.increment_lineno(tree, prefix.count('\n') + 1)
    names = {'require', 'digest', 'remove_staged', 'stage_single_allocation',
             'finalize_evidence', 'record_failure', 'payload_allocation'}
    nodes = [n for n in tree.body if isinstance(n, ast.FunctionDef) and n.name in names]
    ns = dict(Path=Path, hashlib=hashlib, json=json, shutil=shutil, os=os, sys=sys)
    exec(compile(ast.Module(nodes, []), str(WORKFLOW), 'exec'), ns)
    return tree, ns


class EvidenceTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory(prefix='selector-evidence-')
        self.addCleanup(self.temp.cleanup)
        self.root = Path(self.temp.name)
        self.build = self.root / 'owned-build'
        self.build.mkdir()
        self.binary = self.build / 'ripr.exe'
        self.binary.write_bytes(b'compiler payload')
        self.stock = self.root / 'stock'
        self.stock.mkdir()
        self.staged = self.stock / 'ripr.exe'
        self.out = self.root / 'evidence'
        self.out.mkdir()
        self.ownership = {'staged': False, 'target': True}
        self.tree, self.ns = consumer()

    def stage(self):
        if 'stage_single_allocation' in self.ns:
            with patch.object(shutil, 'which', return_value=None):
                return self.ns['stage_single_allocation'](self.binary, self.staged, self.build,
                                                           self.ns['digest'](self.binary), self.out, self.ownership)
        # Pre-repair: execute the real admission and copy statements from Actions.
        body = next(n for n in self.tree.body if isinstance(n, ast.Try)).body
        start = next(i for i, n in enumerate(body) if isinstance(n, ast.Assign)
                     and any(isinstance(t, ast.Name) and t.id == 'staged' for t in n.targets))
        end = next(i for i in range(start, len(body)) if isinstance(body[i], ast.Assign)
                   and any(isinstance(t, ast.Name) and t.id == 'resolved' for t in body[i].targets))
        ns = dict(self.ns, binary=self.binary, stock_bin=self.stock, out=self.out)
        with patch.object(shutil, 'which', return_value=None):
            exec(compile(ast.Module(body[start:end], []), str(WORKFLOW), 'exec'), ns)
        return {'charged_bytes': 2 * self.binary.stat().st_size}

    def finalize(self, receipt, charge):
        if 'finalize_evidence' in self.ns:
            return self.ns['finalize_evidence'](receipt, self.out, None, charge)
        nodes = next(n for n in self.tree.body if isinstance(n, ast.Try)).finalbody
        start = next(i for i, n in enumerate(nodes) if isinstance(n, ast.Assign)
                     and ast.unparse(n.targets[0]) == "receipt['python']")
        ns = dict(self.ns, receipt=receipt, out=self.out, binary=self.binary)
        exec(compile(ast.Module(nodes[start:-2], []), str(WORKFLOW), 'exec'), ns)
        return receipt

    def test_verified_run_arithmetic_and_old_admission_boundary(self):
        self.assertEqual(2 * 67278336, 134556672)
        self.assertEqual(134802028 - 128 * 1024**2, 584300)
        self.assertGreater(2 * 67278336, 128 * 1024**2)

    def test_large_single_allocation_reaches_staging_without_copy(self):
        with self.binary.open('r+b') as f:
            f.truncate(67278336)
        alias = self.build / 'ripr-hash.exe'
        os.link(self.binary, alias)
        identity = self.binary.stat()
        with patch.object(shutil, 'copyfile', side_effect=AssertionError('copy forbidden')):
            result = self.stage()
        self.assertEqual(self.staged.stat().st_ino, identity.st_ino)
        self.assertEqual(self.staged.stat().st_dev, identity.st_dev)
        self.assertEqual(result['charged_bytes'], 67278336)
        self.assertFalse(self.binary.exists())
        self.assertFalse(self.build.exists())
        self.assertEqual(self.staged.stat().st_nlink, 1)
        receipt = self.finalize({'state': 'PASS'}, result['charged_bytes'])
        self.assertEqual(receipt['state'], 'PASS')
        self.assertLess(receipt['final_charged_bytes'], 128 * 1024**2)

    def test_occupied_destination_survives_refusal(self):
        self.staged.write_bytes(b'pre-existing occupant')
        with self.assertRaisesRegex(RuntimeError, 'occupant'):
            self.stage()
        self.assertEqual(self.staged.read_bytes(), b'pre-existing occupant')
        self.assertTrue(self.binary.exists())

    def test_partial_transfer_never_falls_back_to_copy(self):
        with (patch.object(os, 'rename', side_effect=OSError(errno.EXDEV, 'cross-volume')),
              patch.object(shutil, 'copyfile', side_effect=AssertionError('copy forbidden'))):
            with self.assertRaises(OSError):
                self.stage()
        self.assertFalse(self.staged.exists())
        self.assertTrue(self.binary.exists())

    def test_real_cross_volume_device_is_refused_before_rename(self):
        original = Path.stat
        def topology(path, *args, **kwargs):
            info = original(path, *args, **kwargs)
            if path == self.stock:
                values = list(info)
                values[2] = info.st_dev + 1
                return os.stat_result(values)
            return info
        with (patch.object(Path, 'stat', topology),
              patch.object(os, 'rename', side_effect=AssertionError('must not rename'))):
            with self.assertRaisesRegex(RuntimeError, 'cross-volume'):
                self.stage()
        self.assertFalse(self.ownership['staged'])
        self.assertTrue(self.binary.exists())

    def test_producer_digest_drift_is_refused_before_transfer(self):
        before = self.ns['digest'](self.binary)
        self.binary.write_bytes(b'changed producer bytes')
        with self.assertRaisesRegex(RuntimeError, 'digest drift'):
            self.ns['stage_single_allocation'](self.binary, self.staged, self.build,
                                               before, self.out, self.ownership)
        self.assertFalse(self.staged.exists())
        self.assertFalse(self.ownership['staged'])

    def test_post_rename_digest_drift_still_owns_cleanup(self):
        expected = self.ns['digest'](self.binary)
        with patch.dict(self.ns, digest=lambda p: expected if p == self.binary else 'wrong'):
            with self.assertRaisesRegex(RuntimeError, 'drift'):
                self.stage()
        self.assertTrue(self.ownership['staged'])
        self.assertEqual(self.ns['remove_staged'](self.staged, True)['state'], 'REMOVED')

    def test_outside_mutable_alias_is_refused(self):
        os.link(self.binary, self.root / 'outside.exe')
        with self.assertRaisesRegex(RuntimeError, 'unowned executable alias'):
            self.stage()
        self.assertFalse(self.staged.exists())

    def test_separate_cargo_executable_allocation_is_refused(self):
        (self.build / 'ripr-hash.exe').write_bytes(self.binary.read_bytes())
        with self.assertRaisesRegex(RuntimeError, 'separate producer executable allocation'):
            self.stage()
        self.assertFalse(self.staged.exists())

    def test_producer_cleanup_failure_keeps_staged_ownership(self):
        with patch.object(shutil, 'rmtree', side_effect=PermissionError('producer locked')):
            with self.assertRaisesRegex(PermissionError, 'producer locked'):
                self.stage()
        self.assertTrue(self.ownership['staged'])
        self.assertTrue(self.ownership['target'])
        self.assertTrue(self.build.exists())
        self.assertEqual(self.ns['remove_staged'](self.staged, True)['state'], 'REMOVED')

    def test_admission_at_cap_is_refused_without_transfer(self):
        with self.binary.open('r+b') as f:
            f.truncate(128 * 1024**2)
        with self.assertRaisesRegex(RuntimeError, 'budget'):
            self.stage()
        self.assertFalse(self.ownership['staged'])
        self.assertTrue(self.binary.exists())

    def test_final_serialized_charge_is_exact_and_has_boundary(self):
        payload = 65536
        receipt = self.finalize({'state': 'PASS'}, payload)
        charge = payload + 16 + len(json.dumps(receipt, indent=2).encode()) + len(json.dumps(receipt).encode()) + 2
        self.assertEqual(receipt['final_charged_bytes'], charge)
        # A charge at the ceiling is allowed by finalization, one more byte refuses.
        target = 128 * 1024**2
        for _ in range(3):
            receipt = self.finalize({'state': 'PASS'}, payload)
            payload += target - receipt['final_charged_bytes']
        receipt = self.finalize({'state': 'PASS'}, payload)
        self.assertEqual(receipt['final_charged_bytes'], target)
        self.assertEqual(receipt['state'], 'PASS')
        self.assertEqual(self.finalize({'state': 'PASS'}, payload + 1)['error_type'], 'EvidenceBudget')

    def test_cleanup_and_budget_failures_keep_first_reason(self):
        receipt = {'state': 'FAIL', 'error_type': 'Producer', 'error': 'first reason'}
        self.ns['record_failure'](receipt, 'StagingCleanup', 'locked')
        receipt = self.finalize(receipt, 128 * 1024**2)
        self.assertEqual((receipt['primary_error_type'], receipt['primary_error']), ('Producer', 'first reason'))
        self.assertEqual(receipt['error_type'], 'EvidenceBudget')

    def test_cleanup_removes_owned_partial_copy(self):
        self.staged.write_bytes(b'partial')
        result = self.ns['remove_staged'](self.staged, True)
        self.assertEqual(result['state'], 'REMOVED')
        self.assertFalse(self.staged.exists())

    def test_cleanup_failure_is_not_removed(self):
        self.staged.write_bytes(b'owned')
        with patch.object(Path, 'unlink', side_effect=PermissionError('locked')):
            with self.assertRaises(PermissionError):
                self.ns['remove_staged'](self.staged, True)
        self.assertTrue(self.staged.exists())

    def test_final_budget_preserves_original_failure(self):
        with self.binary.open('r+b') as f:
            f.truncate(67278336)
        receipt = self.finalize({'state': 'FAIL', 'error_type': 'Producer', 'error': 'original failure'},
                                2 * 67278336)
        self.assertEqual(receipt['state'], 'FAIL')
        self.assertEqual(receipt['primary_error'], 'original failure')
        self.assertEqual(receipt['primary_error_type'], 'Producer')
        self.assertEqual(receipt['error_type'], 'EvidenceBudget')


if __name__ == '__main__':
    unittest.main(verbosity=2)
