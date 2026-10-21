<!-- section: Fixed -->
- Opening many files in a short burst no longer parks every LSP
  notification slot on the 200ms interactive refresh debounce. Later
  opens join the window already in flight, and the one wakeup analyzes
  the latest workspace revision (#8528).
