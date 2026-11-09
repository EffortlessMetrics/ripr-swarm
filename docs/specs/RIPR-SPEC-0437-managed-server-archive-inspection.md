# RIPR-SPEC-0437: Managed Server Archive Pre-Extraction Inspection

Status: accepted

Owner: product / swarm

Created: 2026-11-09

Linked issues:

- [#13640](https://github.com/EffortlessMetrics/ripr-swarm/issues/13640) - tar
  archive pre-extraction inspection for the managed server download.
- [#13904](https://github.com/EffortlessMetrics/ripr-swarm/issues/13904) - the
  same pre-extraction contract for the Windows zip archive.

## Problem

The VS Code extension's managed server download extracts a published archive
before any member of that archive has been inspected. Post-extraction tree
walks cannot undo writes made through a symlink, `..`, or reparse-point
member: those members must never reach the extractor. The tar path gained
pre-extraction inspection in #13656 (`assertSafeTarListing`); the Windows zip
path must keep the identical contract instead of handing the archive straight
to `Expand-Archive`.

## Behavior

Before any extraction command runs, the extension lists the archive's members
independently of the extractor and refuses the whole archive when any member
violates the policy. Refusal happens before extraction starts; a refused
archive writes nothing. Extraction itself stays delegated to the platform tool
(`tar -xzf` or PowerShell `Expand-Archive`) and runs only after inspection
passes.

The shared policy:

- the member count must not exceed `ARCHIVE_MAX_MEMBERS` (1000);
- names must not be absolute, drive-qualified, contain NUL, or contain a `..`
  path segment in either slash direction.

The tar-specific surface (`assertSafeTarListing` over `tar -tzf` /
`tar -tvzf` output):

- only `-` (regular file) and `d` (directory) member rows are accepted;
- symlink rows, hard-link rows, and rows containing ` link to ` are refused.

The zip-specific surface (`listZipCentralDirectoryEntries` over the central
directory, then `assertSafeZipListing`):

- a missing or out-of-range end-of-central-directory record, a multi-disk
  archive, a central directory that extends past the file, an entry whose
  declared size spills past the directory end, or an entry count that
  disagrees with the directory walk, is a malformed-archive refusal;
- NTFS reparse-point entries (the DOS `FILE_ATTRIBUTE_REPARSE_POINT`
  attribute in the external attributes) are refused;
- Unix modes in the external attributes are classified only when a file type
  is present: symlinks and device/fifo members are refused; permission bits
  without a file type (the shape Windows tooling such as Python
  `zipfile.writestr` writes, e.g. mode `0o600`) describe a regular file and
  are accepted.

## Required Evidence

- Unit tests for both policy validators over fabricated listings, covering
  every refusal rule and the accepted-shape controls (plain files,
  directories, permission-only modes).
- A reader test that feeds hand-built central-directory bytes, including
  truncated and count-mismatched archives, on every platform (pure
  TypeScript, no shell).
- Platform-capable integration proof (windows-advisory or vscode e2e on
  Windows) that a real \`.zip\` archive passes inspection and extraction.
- \`tsc -p .\` clean for the extension.

## Non-Goals

- Download provenance, HTTPS redirect policy, size caps, and the SHA-256
  manifest check (tracked in #1879).
- Replacing the platform extractor with an in-extension extractor.
- Inspection of archives outside the managed server download flow.
- Post-extraction tree verification (\`assertExtractedTreeContained\`), which
  stays as defense in depth and is not replaced by this contract.

## Acceptance Examples

1. A zip whose central directory names \`ripr.exe\` (regular file, no
   attributes) and \`bin/\` (directory) inspects clean and extracts.
2. A zip with one entry whose external attributes carry the Unix symlink
   type (\`0o120000\`) is refused before \`Expand-Archive\` runs, naming the
   member.
3. A zip with one entry whose external attributes are \`0o600 << 16\`
   (permission bits, no file type) inspects clean: Windows-packaged servers
   are not rejected.
4. A zip with a member named \`../../evil\`, \`/etc/passwd\`, \`C:\\evil\`, or
   \`\\evil\` is refused.
5. A tar listing containing an \`l\`-row, an \`h\`-row, or a \` link to \` row is
   refused, as is any listing over 1000 members.
6. A buffer without an end-of-central-directory record, or whose declared
   central directory size exceeds the file, is refused as unreadable or
   malformed.

## Test Mapping

- \`editors/vscode/test/suite/download_hardening.test.ts::assertSafeZipListing
  rejects traversal, symlinks, reparse points, and member floods\` - refusal
  table (examples 4, 5-count, symlink, reparse).
- \`editors/vscode/test/suite/download_hardening.test.ts::assertSafeZipListing
  accepts permission-only modes Windows tools write\` - example 3.
- \`editors/vscode/test/suite/download_hardening.test.ts::listZipCentralDirectoryEntries
  reads names and attributes from a fabricated zip\` - example 1 shape.
- \`editors/vscode/test/suite/download_hardening.test.ts::listZipCentralDirectoryEntries
  refuses truncated or malformed archives\` - example 6.
- \`editors/vscode/test/suite/download_hardening.test.ts::listZipCentralDirectoryEntries
  refuses entries spilling past the directory end\` - malformed entry bound.
- \`editors/vscode/test/suite/download_hardening.test.ts::inspectZipArchive
  accepts a safe fabricated zip and refuses a symlink member\` - examples 1
  and 2 end to end.
- \`editors/vscode/test/suite/download_hardening.test.ts::assertSafeTarListing
  rejects traversal, links, and special members before extraction\` - tar
  surface (example 5).

## Implementation Mapping

- \`editors/vscode/src/downloader.ts::listZipCentralDirectoryEntries\` - pure
  central-directory reader.
- \`editors/vscode/src/downloader.ts::assertSafeZipListing\` - zip policy.
- \`editors/vscode/src/downloader.ts::inspectZipArchive\` - read + validate
  entry point wired into \`extractArchive\` before \`Expand-Archive\`.
- \`editors/vscode/src/downloader.ts::assertSafeTarListing\` and
  \`inspectTarArchive\` - the tar contract this mirrors.
- \`editors/vscode/src/downloader.ts::extractArchive\` - inspection gate for
  both archive kinds.

## Metrics

- No new metric. Archive refusals surface as managed-install failures with
  the existing installation outcome channels; no counter or dashboard is
  added by this contract.
