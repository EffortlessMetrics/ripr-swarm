<!-- section: Fixed -->
- Python: a changed `obj.attr = ...` write on a module-scope object is no longer credited when the test assigns an attribute of that name on a module (`route.response = fake`) or writes a dynamic namespace (`setattr(route, "response", fake)`); the test then reads a different object than the owner mutates (#10178).
