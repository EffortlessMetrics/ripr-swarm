<!-- section: Internal / CI -->
- The scoreboard's servo pin is now the devtools `getFavicon` commit instead of a type-level refactor of `components/net/http_cache.rs`: the old diff has no behavioral candidates, so the same-runner warm A/B correctly refused it ("baseline reported no findings") and the nightly job could never pass on it (#12203).
