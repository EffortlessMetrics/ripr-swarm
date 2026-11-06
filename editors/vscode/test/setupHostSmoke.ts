import * as fs from 'fs';
import * as vscode from 'vscode';

// test-electron's extension-host test entrypoint. This is not the e2e suite.
export async function run(): Promise<void> {
  const nonce = process.env.RIPR_SETUP_HOST_NONCE;
  const receiptPath = process.env.RIPR_SETUP_HOST_RECEIPT;
  const electronVersion = process.versions.electron;
  if (!nonce || !/^[a-f0-9]{32}$/.test(nonce) || !receiptPath || !electronVersion) {
    throw new Error('Setup smoke requires its owned native Electron host and receipt identity');
  }
  fs.writeFileSync(receiptPath, `${JSON.stringify({ schema_version: 1, nonce,
    vscode_version: vscode.version, electron_version: electronVersion,
    platform: process.platform, smoke_cases: 1 })}\n`, { flag: 'wx' });
}
