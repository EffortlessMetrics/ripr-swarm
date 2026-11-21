/**
 * Workspace, trace, and extension-version helpers extracted from client.ts
 * (#2553).
 *
 * These functions read VS Code / language-client APIs but have no dependence
 * on `RiprClientController` instance state, so they are extracted to shrink
 * the controller module and make the workspace-root state shape reviewable on
 * its own. Behavior is unchanged.
 */
import * as vscode from 'vscode';
import { Trace } from 'vscode-languageclient';
import type { RiprConfig } from './config';
import type { RiprWorkspaceRootState } from './client';
import { RIPR_DOCUMENT_SELECTORS } from './client';
import { knownExtensionVersion } from './extensionVersion';

// VS Code matches `pattern` as a glob, so glob metacharacters in the workspace
// path would otherwise change what it matches (`[a]` as a class, `{a,b}` as
// alternation, `*`/`?` as wildcards) and the client would stop seeing events
// for that folder's files. Wrapping each in a bracket class matches it
// literally in VS Code's glob (including `}` outside braces, which would
// otherwise match the empty string).
export function escapeGlobLiteral(text: string): string {
  return text.replace(/[\[\]{}*?]/g, (char) => `[${char}]`);
}

export function riprDocumentSelectorsForWorkspace(
  workspaceRoot: string
): Array<{ language: string; scheme: 'file'; pattern: string }> {
  const workspacePattern = `${escapeGlobLiteral(workspaceRoot.replace(/\\/g, '/'))}/**/*`;
  return RIPR_DOCUMENT_SELECTORS.map((selector) => ({
    ...selector,
    pattern: workspacePattern
  }));
}

// The server reads gitTimeoutMs / refreshDeadlineMs as unsigned integers and
// rejects the whole workspace/configuration pull when one is a fractional
// JSON number, discarding every other ripr setting from that response. A
// non-integer is sent as undefined so the server default applies.
export function integerSetting(value: number | undefined): number | undefined {
  return typeof value === 'number' && Number.isInteger(value) && value >= 0 ? value : undefined;
}

/** The extension version for status and client capabilities; `unknown` only when no manifest can be read (#14647). */
export function extensionVersion(context: vscode.ExtensionContext): string {
  return knownExtensionVersion(context) ?? 'unknown';
}

export function traceFromConfig(trace: RiprConfig['traceServer']): Trace {
  switch (trace) {
    case 'messages':
      return Trace.Messages;
    case 'verbose':
      return Trace.Verbose;
    case 'off':
    default:
      return Trace.Off;
  }
}

/**
 * Quick-pick item for `ripr: Select Workspace Root` (#2077): the folder name
 * as the label and the folder path as the description, so the user can tell
 * same-named folders apart. `root` carries the picked folder back to the
 * controller.
 */
export interface WorkspaceRootPickItem extends vscode.QuickPickItem {
  root: string;
}

/**
 * Build the `ripr: Select Workspace Root` pick list from the workspace
 * folders. Kept pure and exported so the pick-list shape is reviewable and
 * testable without stubbing `vscode.window.showQuickPick` (#2077).
 */
export function workspaceRootPickItems(
  folders: readonly vscode.WorkspaceFolder[]
): WorkspaceRootPickItem[] {
  return folders.map((folder) => ({
    label: folder.name,
    description: folder.uri.fsPath,
    root: folder.uri.fsPath
  }));
}

export function currentWorkspaceRootState(): RiprWorkspaceRootState {
  const folders = vscode.workspace.workspaceFolders ?? [];
  if (folders.length === 0) {
    return workspaceRootStateNoWorkspace();
  }
  if (folders.length === 1) {
    return {
      kind: 'singleRoot',
      root: folders[0].uri.fsPath,
      roots: [folders[0].uri.fsPath],
      detail: 'single workspace folder is active'
    };
  }
  const activeEditor = vscode.window.activeTextEditor;
  const activeFolder = activeEditor && activeEditor.document.uri.scheme === 'file'
    ? vscode.workspace.getWorkspaceFolder(activeEditor.document.uri)
    : undefined;
  if (activeFolder) {
    return {
      kind: 'selectedRoot',
      root: activeFolder.uri.fsPath,
      roots: folders.map((folder) => folder.uri.fsPath),
      detail: 'selected from active editor workspace folder'
    };
  }
  return {
    kind: 'ambiguousMultiRoot',
    roots: folders.map((folder) => folder.uri.fsPath),
    detail: 'multiple workspace folders are open and no active editor selected a safe root'
  };
}

/** Project-root markers that make ripr analysis possible without extra setup.
 * Python list mirrors `PYTHON_PROJECT_MARKERS` in `crates/ripr/src/config/python.rs`. */
export const PYTHON_PROJECT_MARKERS = [
  'pyproject.toml',
  'setup.py',
  'setup.cfg',
  'requirements.txt',
  'pytest.ini',
  'tox.ini',
  'noxfile.py',
] as const;

const RUST_OR_RIPR_PROJECT_MARKERS = ['Cargo.toml', 'ripr.toml'] as const;

/** Matches the README: tsconfig/jsconfig enable TypeScript; package.json alone does not. */
export const TYPESCRIPT_PROJECT_MARKERS = ['tsconfig.json', 'jsconfig.json'] as const;

/** Every root marker `workspaceHasAnalyzableProject` accepts; `activationEvents` lists the same set (#14647). */
export const ANALYZABLE_PROJECT_MARKERS: readonly string[] = [
  ...RUST_OR_RIPR_PROJECT_MARKERS,
  ...PYTHON_PROJECT_MARKERS,
  ...TYPESCRIPT_PROJECT_MARKERS,
];

async function folderRootHasAnalyzableProjectMarker(
  folderUri: vscode.Uri
): Promise<boolean> {
  for (const marker of ANALYZABLE_PROJECT_MARKERS) {
    try {
      const stat = await vscode.workspace.fs.stat(vscode.Uri.joinPath(folderUri, marker));
      if ((stat.type & vscode.FileType.File) !== 0) {
        return true;
      }
    } catch {
      // missing or unreadable marker
    }
  }
  return false;
}

// A repository often keeps its project one or two folders down
// (`backend/Cargo.toml`, `packages/app/tsconfig.json`), and the server
// analyzes it from the repository root. The nested check is presence-only and
// bounded, so a large tree cannot slow activation (#15916).
export const NESTED_PROJECT_MARKER_MAX_DEPTH = 3;
export const NESTED_PROJECT_MARKER_MAX_DIRS = 512;
// Narrower than the root set: below the root, `requirements.txt`, `tox.ini`
// and `noxfile.py` sit in docs and tooling folders of projects in other
// languages, and a nested `ripr.toml` does not configure the opened root.
// The Python names mirror `NESTED_PYTHON_PACKAGE_MARKERS` in
// `crates/ripr/src/config/python.rs`.
export const NESTED_PROJECT_MARKERS: ReadonlySet<string> = new Set([
  'Cargo.toml',
  'pyproject.toml',
  'setup.py',
  'setup.cfg',
  'pytest.ini',
  ...TYPESCRIPT_PROJECT_MARKERS,
]);
// Dependency, build and environment folders: a marker inside one belongs to
// another project. Mirrors `PYTHON_EXCLUDED_DIRS` plus `vendor` in the same
// server file; hidden folders are skipped by name.
const NESTED_PROJECT_MARKER_SKIPPED_DIRS: ReadonlySet<string> = new Set([
  'target',
  'node_modules',
  '__pycache__',
  'venv',
  'env',
  'site-packages',
  'dist',
  'build',
  'vendor',
]);

async function folderHasNestedAnalyzableProjectMarker(folderUri: vscode.Uri): Promise<boolean> {
  let level = [folderUri];
  let visited = 0;
  // Depth 0 is the folder root, whose own markers the caller already checked.
  for (let depth = 0; depth <= NESTED_PROJECT_MARKER_MAX_DEPTH && level.length > 0; depth++) {
    const next: vscode.Uri[] = [];
    for (const dir of level) {
      let entries: [string, vscode.FileType][];
      try {
        entries = await vscode.workspace.fs.readDirectory(dir);
      } catch {
        continue;
      }
      for (const [name, type] of entries) {
        if (depth > 0 && (type & vscode.FileType.File) !== 0 && NESTED_PROJECT_MARKERS.has(name)) {
          return true;
        }
        // A symlinked folder is not entered: it can point back up the tree.
        if (
          type !== vscode.FileType.Directory ||
          name.startsWith('.') ||
          NESTED_PROJECT_MARKER_SKIPPED_DIRS.has(name)
        ) {
          continue;
        }
        if (visited >= NESTED_PROJECT_MARKER_MAX_DIRS) {
          return false;
        }
        visited += 1;
        next.push(vscode.Uri.joinPath(dir, name));
      }
    }
    level = next;
  }
  return false;
}

/**
 * True when any workspace folder carries a marker ripr can analyze by default:
 * at its root, or in a folder up to `NESTED_PROJECT_MARKER_MAX_DEPTH` levels
 * below it.
 */
export async function workspaceHasAnalyzableProject(
  folders: readonly vscode.WorkspaceFolder[]
): Promise<boolean> {
  for (const folder of folders) {
    if (await folderRootHasAnalyzableProjectMarker(folder.uri)) {
      return true;
    }
  }
  for (const folder of folders) {
    if (await folderHasNestedAnalyzableProjectMarker(folder.uri)) {
      return true;
    }
  }
  return false;
}

export function workspaceRootStateNoWorkspace(): RiprWorkspaceRootState {
  return {
    kind: 'noWorkspace',
    roots: [],
    detail: 'open a workspace folder before matching saved-workspace artifacts'
  };
}

export function workspaceRootStateLabel(state: RiprWorkspaceRootState): string {
  switch (state.kind) {
    case 'singleRoot':
      return `workspace_single_root (${state.root ?? 'unknown'})`;
    case 'selectedRoot':
      return `workspace_multi_root_selected (${state.root ?? 'unknown'}; roots: ${state.roots.join(', ')})`;
    case 'ambiguousMultiRoot':
      return `workspace_multi_root_ambiguous (roots: ${state.roots.join(', ') || 'unknown'})`;
    case 'noWorkspace':
    default:
      return 'workspace_not_open';
  }
}

export function workspaceRootStateDetail(state: RiprWorkspaceRootState): string {
  const lines = [
    state.detail ?? 'workspace root state is unavailable'
  ];
  if (state.roots.length > 0) {
    lines.push(`Workspace folders: ${state.roots.join(', ')}`);
  }
  lines.push('Root-scoped repair actions are suppressed until one workspace folder is selected.');
  return lines.join('\n');
}
