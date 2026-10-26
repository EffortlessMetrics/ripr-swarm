<!-- section: Fixed -->
- Repo-mode Python findings no longer offer a repair card, and repo-mode TypeScript/JavaScript findings no longer offer a repair packet. These languages register repair routing for diff mode only, so repo-mode findings now fail closed; diff-mode routing is unchanged (#5531, #9896, #9923).
