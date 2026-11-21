<!-- section: Fixed -->
- `ripr check`: when the changed line sits in a macro body, an attribute or an item outside a function, the Next line now says ripr cannot trace it and points at the tests that use the macro, attribute or item. It used to ask you to move the line into a function; the report-the-shape route stays for a line inside an ordinary function (#15412).
