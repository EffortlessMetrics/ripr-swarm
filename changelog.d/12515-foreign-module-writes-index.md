<!-- section: Fixed -->
- The Python warm check no longer scans every workspace attribute write for every test (tests x writes), which made `ripr check` on pandas and django about four times slower (#12515).
