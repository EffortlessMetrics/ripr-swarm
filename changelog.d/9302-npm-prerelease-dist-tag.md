<!-- section: Fixed -->
- Fails the distribution contract check when the npm launcher is a prerelease without an explicit non-`latest` `publishConfig.tag`, or when a workflow runs `npm publish` without a non-`latest` `--tag` or moves `latest` with `npm dist-tag` (#9302).
