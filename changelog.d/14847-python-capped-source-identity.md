<!-- section: Fixed -->
- Python repo analysis now includes source paths beyond the working-set limit
  when checking whether a plain module shadows a src module. Omitted files
  remain outside the analyzed set, and the run still reports capped (#14847).
