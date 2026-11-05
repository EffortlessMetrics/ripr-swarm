import * as fs from 'fs';
import * as path from 'path';
import { downloadVSCodeWithRetry, requireEditorDisplay, resetEditorSetupEvidence, verifyHostPrerequisites } from './hostPrerequisites';
import { redactHostDiagnosticsPrefix } from './hostDiagnostics';

export interface EditorSetupRuntime {
  requireEditorDisplay: typeof requireEditorDisplay;
  downloadVSCode: typeof downloadVSCodeWithRetry;
  verifyHostPrerequisites: typeof verifyHostPrerequisites;
  report: (record: Record<string, unknown>, failed: boolean) => void;
}

const defaultEditorSetupRuntime: EditorSetupRuntime = {
  requireEditorDisplay, downloadVSCode: downloadVSCodeWithRetry, verifyHostPrerequisites,
  report: (record, failed) => {
    const text = JSON.stringify(record, null, 2);
    if (failed) console.error(text); else console.info(text);
  }
};

// Explicit setup verification, reached only by setup:verify-editor. The cheap
// source frontdoor is deliberately independent of this compiled/network path.
export async function verifyEditorSetup(runtime: EditorSetupRuntime = defaultEditorSetupRuntime): Promise<void> {
  const extensionRoot = path.resolve(__dirname, '../..');
  const artifactRoot = path.resolve(process.env.RIPR_TEST_ARTIFACT_ROOT
    ?? path.join(extensionRoot, '../../target/ripr'));
  const phases = { acquisition: 'not_run', native_host: 'not_run',
    genuine_producer: 'not_run', canonical_editor_suite: 'not_run',
    full_development_validation: 'not_run', fresh_task: 'not_proven' };
  try {
    resetEditorSetupEvidence(artifactRoot);
    runtime.requireEditorDisplay(process.platform, process.env);
    const cache = path.join(artifactRoot, 'vscode-test-cache');
    fs.mkdirSync(cache, { recursive: true });
    phases.acquisition = 'running';
    const executable = await runtime.downloadVSCode(cache, extensionRoot);
    phases.acquisition = 'pass';
    phases.native_host = 'running';
    const receipt = await runtime.verifyHostPrerequisites(executable, artifactRoot);
    phases.native_host = 'pass';
    runtime.report({ kind: 'ripr.developer-setup', schema_version: 1,
      mode: 'explicit_editor_verification', phases, receipt }, false);
  } catch (error) {
    for (const phase of ['acquisition', 'native_host'] as const) {
      if (phases[phase] === 'running') phases[phase] = 'fail';
    }
    runtime.report({ kind: 'ripr.developer-setup', schema_version: 1,
      mode: 'explicit_editor_verification', phases,
      error: redactHostDiagnosticsPrefix(String(error), 2048).toString('utf8') }, true);
    throw error;
  }
}

if (require.main === module) {
  verifyEditorSetup().catch(() => { process.exitCode = 1; });
}
