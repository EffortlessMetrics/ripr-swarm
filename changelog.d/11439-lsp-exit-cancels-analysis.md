<!-- section: Fixed -->
- The editor sidecar now cancels in-flight analysis when the client closes its input or sends `exit` without waiting for the `shutdown` response, so restarting an editor no longer waits for a long analysis pass to finish before the old sidecar exits (#11439).
