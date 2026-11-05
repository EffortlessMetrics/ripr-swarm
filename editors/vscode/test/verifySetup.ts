import * as fs from 'fs';
import * as path from 'path';
import { downloadVSCodeWithRetry, requireEditorDisplay, verifyHostPrerequisites } from './hostPrerequisites';

// Explicit setup verification, reached only by setup:verify-editor. The cheap
// source frontdoor is deliberately independent of this compiled/network path.
export async function verifyEditorSetup(): Promise<void> {
  const extensionRoot = path.resolve(__dirname, '../..');
  const artifactRoot = path.resolve(process.env.RIPR_TEST_ARTIFACT_ROOT
    ?? path.join(extensionRoot, '../../target/ripr'));
  const phases = { acquisition: 'not_run', native_host: 'not_run',
    genuine_producer: 'not_run', canonical_editor_suite: 'not_run',
    full_development_validation: 'not_run', fresh_task: 'not_proven' };
  try {
    requireEditorDisplay(process.platform, process.env);
    const cache = path.join(artifactRoot, 'vscode-test-cache');
    fs.mkdirSync(cache, { recursive: true });
    phases.acquisition = 'running';
    const executable = await downloadVSCodeWithRetry(cache, extensionRoot);
    phases.acquisition = 'pass';
    phases.native_host = 'running';
    const receipt = await verifyHostPrerequisites(executable, artifactRoot);
    phases.native_host = 'pass';
    console.info(JSON.stringify({ kind: 'ripr.developer-setup', schema_version: 1,
      mode: 'explicit_editor_verification', phases, receipt }, null, 2));
  } catch (error) {
    for (const phase of ['acquisition', 'native_host'] as const) {
      if (phases[phase] === 'running') phases[phase] = 'fail';
    }
    console.error(JSON.stringify({ kind: 'ripr.developer-setup', schema_version: 1,
      mode: 'explicit_editor_verification', phases,
      error: String(error).slice(0,2048) }, null, 2));
    throw error;
  }
}

if (require.main === module) {
  verifyEditorSetup().catch(() => { process.exitCode = 1; });
}
