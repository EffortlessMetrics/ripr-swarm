<!-- section: Internal / CI -->
- Routed admission binds a merge-queue commit to the live PR head by tree (head merged onto the group base) instead of a commit-date comparison, refusing a different head, extra content, a merge conflict or a moved PR ref (#4836).
