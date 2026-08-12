import { LspCompatibilityEvidence } from '../../src/lspCompatibility';

export const compatibleLspEvidence: LspCompatibilityEvidence = {
  status: 'compatible',
  positionEncoding: 'utf-16',
  required: { textDocumentSync: true, hover: true, codeAction: true, positionEncoding: true },
  optional: {
    pullDiagnostics: true,
    codeActionResolve: true,
    executeCommand: true,
    workspaceFolders: true,
    workDoneProgress: false
  },
  processResult: 'clean_exit'
};
