<!-- section: Internal / CI -->
- The routed Result's late eligibility check now refuses a superseded invocation: a newer run of the same request kind on the same head or queue-entry commit, or a newer attempt of the same run, revokes the older one, and any unreadable, truncated or ambiguous run inventory refuses. Triggers, permissions and the required context are unchanged (#4986, #8082).
