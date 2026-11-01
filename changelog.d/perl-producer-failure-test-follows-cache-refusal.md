<!-- section: Internal / CI -->
- The two Perl producer-failure tests in `preview_analyzed_outcome.rs` (lang-perl only, so only Test Analytics runs them) accept either the cache-containment refusal added in #11601 or the original `create_dir_all` failure as the real cause, keeping the check that the config advice is not repeated (#11822).
