<!-- section: Fixed -->
- The language server no longer reuses a cached on-disk digest for a file modified within the last two seconds, so a same-length rewrite inside one filesystem timestamp tick is no longer judged equal to the previous contents (#12046).
