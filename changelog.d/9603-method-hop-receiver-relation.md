<!-- section: Fixed -->
- Receiver-name coincidences no longer hide a test's real helper relation
  when a free-function owner is reached through a method hop. Genuine bare
  and module-qualified free calls keep their direct-owner relation (#9603).
