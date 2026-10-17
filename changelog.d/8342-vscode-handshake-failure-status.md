<!-- section: Fixed -->
- The VS Code extension no longer leaves the status bar on "starting" when the resolved server dies at the LSP handshake. It now shows the server-unavailable status with the reason and a Restart Server next step plus the Show Output warning, and activation logs the failure instead of rejecting (#8342).
