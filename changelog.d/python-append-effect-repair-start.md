<!-- section: Fixed -->
- A Python preview finding whose changed line appends to a container
  (for example `buf.append(msg.upper())`) and whose only related test
  observes that container through `len(...)` now names the appended
  contents as its missing check and can start an agent repair bound to
  the selected test, instead of refusing as not repairable. The repair
  strengthens that one test to assert the contents; it does not promote
  the finding to exposed (#12903).
