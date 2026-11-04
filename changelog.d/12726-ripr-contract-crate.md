<!-- section: Internal / CI -->
- Found `ripr-contract`, an internal support crate published in lockstep with
  `ripr` (ADR 0027): the domain DTOs and value-origin shells moved out of the
  monolithic `ripr` rustc unit behind the unchanged `ripr::domain` facade, as
  the first measured step toward the 8 GB largest-rustc target (#12726, #8382).
