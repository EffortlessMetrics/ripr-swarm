<!-- section: Fixed -->
- A Python dict or list element change no longer reads `exposed` when the only
  strong oracle subscripts an unchanged sibling whose value equals the changed
  literal (`assert build()['timeout'] == 80` after `'port': 8080` became `80`)
  (RIPR-SPEC-0238 example 22, #6585).
