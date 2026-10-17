<!-- section: Fixed -->
- Python: a `@classmethod` or `@staticmethod` owner called on its imported
  class (`Service.run(h)`) now reaches rule 17's mock credit, both through
  the local the test passed and through the owner's result when the owner
  returns that parameter. Other decorators still keep the result
  uncredited (RIPR-SPEC-0238 rule 17, #7634).
