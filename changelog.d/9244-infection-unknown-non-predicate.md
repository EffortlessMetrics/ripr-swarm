<!-- section: Fixed -->
- The next step for an `infection_unknown` finding on a scalar return value, error variant or other non-predicate expression no longer asks which side of a condition the test exercises; it asks what the changed expression produces for each test input, and match-arm findings ask which arm each input selects (#9244).
