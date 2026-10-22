<!-- section: Fixed -->
- `ripr agent verify` compiles again on main: the foreign-producer refusal added by #9632 referenced a `subject` that #9547's shared identity validator no longer defined. The check-findings route now also refuses a foreign producer version, as #9632 intended (#9500).
