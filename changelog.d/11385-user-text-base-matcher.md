<!-- section: Internal / CI -->
- Hosted PR and merge-queue runs now judge the PR's `human.txt` goldens a second time with the user-text matcher built from the merge-base (`check-user-text --tree`), so a PR cannot weaken the matcher or its exemptions to qualify its own wording change (#11385).
