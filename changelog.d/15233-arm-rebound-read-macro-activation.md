<!-- section: Fixed -->
- A repo-exposure `match` arm that no visible test input selects no longer
  reads `activate: yes` (and `strongly_gripped`) when a rebound read macro
  (`assert_eq!`, `println!`) leaves it unnamed: its activation now reads
  `unknown`, since the inputs were read through a macro whose expansion ripr
  does not read (#15233).
