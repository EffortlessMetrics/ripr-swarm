<!-- section: Fixed -->
- Fixed a Windows-only refusal where a finished gap-bound repair attempt reported its gap snapshot as not bound to the receipt: a verbatim `\\?\D:\` root and a plain `D:` root now compare as the same place, and the refusal names which binding check failed (#11444).
