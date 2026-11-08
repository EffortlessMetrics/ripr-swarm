<!-- section: Fixed -->
- TypeScript preview: row-local changed-sink observation is tri-state (#5528),
  so related-test rows can carry `assertion_not_observing` only when another
  resolved sink is established and keep unresolved flows on
  `observation_unconfirmed`. Aggregate exposure classification is unchanged.
