<!-- section: Internal / CI -->
- The frozen UB #1440 source behind the `uncertain_delivery_helper` witness
  is now `github_delivery.rs.txt`, so ripr no longer indexes it as a Rust file
  of this workspace and its `ensure!` calls stop blocking assertion credit on
  ripr's own changes (#14116).
