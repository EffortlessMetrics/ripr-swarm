<!-- section: Fixed -->
- A repair attempt interrupted between linking an artifact into place and removing its `.tmp-<pid>-<nonce>` staging name no longer leaves the repository permanently ambiguous: the edit cage accepts exactly that two-name pair inside one directory, while any other extra hard link stays refused (#12791).
