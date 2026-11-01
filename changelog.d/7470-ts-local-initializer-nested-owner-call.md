<!-- section: Fixed -->
- TypeScript: a local bound to an owner call that nests another owner call (`const result = price(price(6000, 0), 7)`) now reads the outer call's arguments, as the direct `expect(price(price(6000, 0), 7))` form already did, so the dropped-term and instance-class pin credits are no longer withheld from it (#7470).
