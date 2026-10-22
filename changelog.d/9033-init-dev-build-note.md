<!-- section: Changed -->
- `ripr init --ci github` on a development build now explains calmly that the generated workflow installs the latest published release because this build is a development version, and that nothing is needed until the build is published, after which to upgrade ripr and then refresh; the pinned version is unchanged (#9033).
