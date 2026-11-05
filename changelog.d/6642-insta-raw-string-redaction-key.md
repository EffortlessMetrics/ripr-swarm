<!-- section: Fixed -->
- Read a raw-string insta redaction key (`r#".id"# => ..`) as a redaction, so the redacted field keeps its gap as an ordinary-string key does, and stop reading a match arm in the snapshotted value as a redaction selector; an unredacted sibling beside a map stays withheld (#6642).
- Preserve UTF-8 cursor boundaries in Insta redaction selectors and continue after nested redaction values so later redacted fields keep their gap (#6642).
