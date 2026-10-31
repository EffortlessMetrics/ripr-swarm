<!-- section: Changed -->
- `ripr pilot` now writes at most 3 packets to `agent-seam-packets.json` by default, actionable packets first, instead of 2-4 MB of mostly static-limitation packets; the file reports the cut in `limitations[]`, and `RIPR_PILOT_PACKET_BUDGET=0` writes every packet (#11951).
