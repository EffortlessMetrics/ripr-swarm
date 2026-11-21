<!-- section: Fixed -->
- The generated workflow's job summary now shows the start-here and first-action commands and paths from the repository root (`git -C . fetch ...`, `--root .`) instead of the runner's checkout path, so they paste into a local checkout. The uploaded reports keep the real path (#15858).
