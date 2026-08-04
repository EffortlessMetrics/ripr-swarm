import * as vscode from 'vscode';

export type TraceSetting = 'off' | 'messages' | 'verbose';
export type DiagnosticProfile = 'actionable' | 'full';

export interface RiprConfig {
  enabled: boolean;
  serverPath: string;
  serverArgs: string[];
  autoDownload: boolean;
  serverVersion: string;
  downloadBaseUrl: string;
  checkMode: 'instant' | 'draft' | 'fast' | 'deep' | 'ready';
  baseRef: string;
  seamDiagnostics: boolean;
  diagnosticProfile: DiagnosticProfile;
  traceServer: TraceSetting;
}

export function getConfig(): RiprConfig {
  const config = vscode.workspace.getConfiguration('ripr');
  return {
    enabled: config.get<boolean>('enabled', true),
    serverPath: config.get<string>('server.path', ''),
    serverArgs: config.get<string[]>('server.args', ['lsp', '--stdio']),
    autoDownload: config.get<boolean>('server.autoDownload', true),
    serverVersion: config.get<string>('server.version', ''),
    downloadBaseUrl: config.get<string>('server.downloadBaseUrl', ''),
    checkMode: config.get<'instant' | 'draft' | 'fast' | 'deep' | 'ready'>('check.mode', 'draft'),
    baseRef: config.get<string>('baseRef', 'origin/main'),
    seamDiagnostics: config.get<boolean>('seamDiagnostics', true),
    diagnosticProfile: config.get<DiagnosticProfile>('diagnosticProfile', 'actionable'),
    traceServer: config.get<TraceSetting>('trace.server', 'off')
  };
}
