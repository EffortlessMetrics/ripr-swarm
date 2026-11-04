<!-- section: Internal / CI -->
- Hosted test shards add 8 GB of swap before the test build and the memory trace now reports swap use. The `ripr` lib test compile alone holds about 11 GB before codegen starts and peaks near 14 GB, and the compile split left only 1.1 to 2.2 GB free on a 16 GB runner (#12719). No check is relaxed; a swap setup failure is logged and the build still runs.
