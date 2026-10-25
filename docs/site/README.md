# Release website

The landing page is plain HTML and CSS. Serve `index.html`, `styles.css`, and
`icon.svg` together; these three files are the complete static artifact. Relative
asset paths also support previews under a subdirectory. No JavaScript, package
installation, external fonts, analytics, or application server is required.

## Maintain the content

Keep the existing documentation as the source of truth:

- [Product explanation](../../README.md) and [documentation index](../README.md).
- [Quickstart](../QUICKSTART.md) and [installation channels](../INSTALL_CHANNELS.md).
- [Static exposure model](../STATIC_EXPOSURE_MODEL.md), [limits](../STATIC_LIMITS.md),
  and [support tiers](../status/SUPPORT_TIERS.md).
- [Release copy checklist](../RELEASE_COPY_CHECKLIST.md).

The public repository is `https://github.com/EffortlessMetrics/ripr`; development
is in `https://github.com/EffortlessMetrics/ripr-swarm`. The page separates the
published 0.10.0 install from the explicitly labeled 0.11 development fixture
output. Keep the example tied to real checked output and link to the matching
version's installation and usage documentation.

Before changing the install version, verify the published registry package and
release assets, then update the command, prerequisites, download link, and
versioned usage guide together. A workspace version bump does not establish
public availability. Check that linked public documentation contains the section
and instructions described by the link text. Local files can be ahead of the
public source.

The [brand icon](../../assets/logo/ripr-icon-light.svg) is copied unchanged as
`icon.svg` so the artifact can be served independently of the repository.
Refresh this copy when that source asset changes; see the
[brand guide](../../assets/logo/README.md).

## Preview and validate

From the repository root:

```bash
python3 -m http.server 4173 --bind 127.0.0.1 --directory docs/site
```

Open `http://127.0.0.1:4173/`. There is no compilation step. To stage the static
artifact, copy exactly the three files into an empty directory and compare their
SHA-256 hashes with the sources.

For a website change:

- Run `cargo test -p xtask --bin xtask release_website_tests`. It checks local
  links/assets and fixture excerpts, then executes the page's focused equality
  test in a standalone project using the boundary fixture's function and tests.
  This models the reader's crate, not ripr's nested fixture workspace. Both
  original tests must pass with the old condition, while the equality test must
  reject it; a successful command that executes zero tests is also rejected. The matching
  development build is required to replay the displayed ripr fixture command;
  the page's 0.10.0 install remains a separate published-version workflow.
- Validate HTML and run a browser accessibility scan such as axe-core. Automated
  scans provide partial accessibility evidence.
- Check desktop and mobile layouts at 320, 375, 768, and 1440 CSS pixels, 200%
  text sizing, reduced motion, and forced colors. Inspect desktop and mobile
  screenshots for clipped text or horizontal page overflow.
- Use the keyboard to expose the skip link, focus the main content, and follow
  navigation. Check link names, focus indicators, and contrast.
- Request every local asset, follow in-page anchors, and inspect browser errors.
  Check relative assets under a preview subdirectory.
- Read back external links and their claimed sections against the intended
  public version. A local path check does not establish remote availability.
- Follow the repository's native validation procedure for the changed files,
  including `cargo xtask markdown-links`, `cargo xtask check-doc-index`,
  `cargo xtask check-static-language`, `cargo xtask check-fast`, and
  `cargo xtask precommit`. Use the actual candidate base for attribution.

The static artifact is independent of its hosting configuration. Keep hosting,
custom-domain setup, credentials, and deployment workflows outside these files.
