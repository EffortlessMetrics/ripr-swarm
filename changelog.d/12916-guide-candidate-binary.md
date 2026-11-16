<!-- section: Internal / CI -->
- Guide tests now build the default-feature candidate binary once into the private `target/guide-candidate` directory instead of rebuilding it in the shared target directory in every nextest process, and CI shares that one prepared binary with all guide test processes through `RIPR_GUIDE_CANDIDATE_BINARY` (#12916).
