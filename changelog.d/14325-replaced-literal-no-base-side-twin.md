<!-- section: Fixed -->
- A changed bare literal such as `1200` -> `1500` no longer adds a base-side `static_unknown` twin for the removed value. That twin no test can reach kept `verify=refused` after the new value was fully exposed (#14325).
