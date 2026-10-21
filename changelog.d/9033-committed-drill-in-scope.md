<!-- section: Fixed -->
- Commands printed by a committed check retain `--committed` through explain, context and reciprocal navigation, while unqualified dirty-tree inspection keeps its worktree default. Both inspection commands accept the explicit committed source and reject conflicting worktree, diff or artifact sources before input reads (#9033).
