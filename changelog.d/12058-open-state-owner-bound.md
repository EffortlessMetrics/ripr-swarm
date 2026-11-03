<!-- section: Fixed -->
- The agent format's `open=` state no longer fires for an unrelated weak or smoke test that is related to a reach-limited finding only by file or module proximity; such a finding keeps `stop=`. A weak row bound to the changed owner (direct or helper call, owner-named test) still opens it (#12058).
