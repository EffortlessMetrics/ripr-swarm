<!-- section: Fixed -->
- Agent packet assertion templates for a field write (`self.timeout = t * 2`)
  follow how the test reads the field: an assertion on the receiver, on the
  call result, or a neutral comment when the reading is unspecified, instead of
  always suggesting an assertion on a returned struct (#11726, #12277).
