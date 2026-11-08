<!-- section: Fixed -->
- A TypeScript preview finding whose related test already has a strong exact-value or snapshot check now says the check is strong but cannot be tied to the changed line and asks for an input that flips the branch, instead of asking for another exact-value assertion; a field update that no input can flip keeps the generic step (RIPR-SPEC-0027, #12463).
