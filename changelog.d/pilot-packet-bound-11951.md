<!-- section: Changed -->
- `ripr pilot` now writes at most 3 packets to `agent-seam-packets.json` by default, actionable packets first, instead of 2-4 MB of mostly static-limitation packets; the file reports the cut in `limitations[]`, and `RIPR_PILOT_PACKET_BUDGET=0` writes every packet (#11951).
- In the `ripr pilot` copy of `agent-seam-packets.json`, `packets_total` now means the number of packets written, not the number of packet-eligible seams; `limitations[].packets_eligible` carries the eligible count when the bound cuts (#11951).
