"""Disposable demonstration of the reviewed-slice readback checks.

Run: python3 -m unittest discover -s tests/git -p 'test_*.py'
This exercises Git semantics; it does not authorize a transfer or a join.
"""
import subprocess
import tempfile
import unittest
from pathlib import Path


def git(cwd, *args, data=None):
    return subprocess.run(['git', '-C', str(cwd), *args], input=data,
                          stdout=subprocess.PIPE, stderr=subprocess.PIPE,
                          check=True).stdout


def verify(repo, ref, head, parent, tree, raw):
    """Compare independently recorded identities and the complete raw diff."""
    actual = git(repo, 'rev-parse', '--verify', ref + '^{commit}').strip()
    assert actual == head
    parents = git(repo, 'show', '-s', '--format=%P', head.decode()).split()
    assert parents == [parent]
    assert git(repo, 'rev-parse', head.decode() + '^{tree}').strip() == tree
    assert git(repo, 'diff', '--raw', '-z', '--no-renames', '--no-abbrev',
               parent.decode(), head.decode()) == raw


class CarrierReadback(unittest.TestCase):
    def test_readback_and_refusals(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            producer, remote, consumer = (root / n for n in ('producer', 'remote', 'consumer'))
            subprocess.run(['git', 'init', '-q', '--bare', str(remote)], check=True)
            subprocess.run(['git', 'init', '-q', str(producer)], check=True)
            git(producer, 'config', 'user.name', 'Test')
            git(producer, 'config', 'user.email', 'test@example.invalid')
            (producer / 'gone').write_bytes(b'delete\n')
            (producer / 'keep').write_bytes(b'base\n')
            git(producer, 'add', '.')
            git(producer, 'commit', '-qm', 'base')
            base = git(producer, 'rev-parse', 'HEAD').strip()
            (producer / 'gone').unlink()
            (producer / 'keep').write_bytes(b'changed\x00bytes\n')
            (producer / 'new\nfile').write_bytes(b'new\n')
            git(producer, 'add', '-A')
            git(producer, 'commit', '-qm', 'slice')
            head = git(producer, 'rev-parse', 'HEAD').strip()
            tree = git(producer, 'rev-parse', 'HEAD^{tree}').strip()
            raw = git(producer, 'diff', '--raw', '-z', '--no-renames', '--no-abbrev', base.decode(), head.decode())
            # Independent setup assertions prevent an empty or incomplete subject.
            entries = raw.rstrip(b'\x00').split(b'\x00')
            changes = dict(zip(entries[1::2], entries[::2]))
            self.assertEqual(set(changes), {b'gone', b'keep', b'new\nfile'})
            self.assertTrue(changes[b'gone'].endswith(b' D'))
            self.assertTrue(changes[b'keep'].endswith(b' M'))
            self.assertTrue(changes[b'new\nfile'].endswith(b' A'))
            self.assertTrue(changes[b'new\nfile'].startswith(b':000000 100644 '))
            git(producer, 'remote', 'add', 'origin', str(remote))
            git(producer, 'push', '-q', 'origin', 'HEAD:refs/heads/carrier')
            subprocess.run(['git', 'clone', '-q', '--no-checkout', str(remote), str(consumer)], check=True, stderr=subprocess.PIPE)
            git(consumer, 'fetch', '-q', 'origin', 'refs/heads/carrier:refs/remotes/origin/carrier')
            ref = 'refs/remotes/origin/carrier'
            verify(consumer, ref, head, base, tree, raw)
            self.assertEqual(git(consumer, 'cat-file', 'blob', head.decode() + ':keep'), b'changed\x00bytes\n')
            self.assertEqual(git(consumer, 'cat-file', 'blob', head.decode() + ':new\nfile'), b'new\n')
            with self.assertRaises(subprocess.CalledProcessError):
                git(consumer, 'cat-file', '-e', head.decode() + ':gone')
            merge = git(producer, 'commit-tree', tree.decode(), '-p', base.decode(), '-p', head.decode(), data=b'merge\n').strip()
            git(producer, 'update-ref', 'refs/heads/merge-test', merge.decode())
            git(producer, 'push', '-q', 'origin', 'refs/heads/merge-test')
            git(consumer, 'fetch', '-q', 'origin', 'refs/heads/merge-test:refs/remotes/origin/merge-test')
            with self.assertRaises(AssertionError):
                verify(consumer, 'refs/remotes/origin/merge-test', merge, base, tree, raw)
            # Each independent mismatch must reject rather than accept partial readback.
            for h, p, t, changes in [(base, base, tree, raw), (head, head, tree, raw),
                                      (head, base, base, raw), (head, base, tree, raw.replace(b'gone\x00', b'other\x00')),
                                      (head, base, tree, raw.replace(b'100644', b'100755', 1)),
                                      (head, base, tree, raw[:raw.index(b'gone\x00') - 1])]:
                with self.assertRaises(AssertionError):
                    verify(consumer, ref, h, p, t, changes)
            git(consumer, 'update-ref', ref, base.decode())
            with self.assertRaises(AssertionError):
                verify(consumer, ref, head, base, tree, raw)


if __name__ == '__main__':
    unittest.main()
