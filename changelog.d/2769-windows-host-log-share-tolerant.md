<!-- section: Internal / CI -->
- The Windows packaged editor journeys retain the VS Code host logs with a share-tolerant read, so a hung or timed-out host that still holds its log files no longer leaves the receipts without host logs (rehearsal run 39720121179 timed out and retained none) (#2769).
