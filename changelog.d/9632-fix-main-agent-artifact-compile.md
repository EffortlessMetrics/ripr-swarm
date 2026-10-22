<!-- section: Fixed -->
- `agent verify` builds again: the foreign-producer refusal in the artifact
  identity check used a variable only the other validator defines, so main did
  not compile after #9632 met #9547 (#9500).
