<!-- section: Fixed -->

- A test file whose `use` binds the owner type's name to a different type (for example `use other_crate::Regex;`) no longer earns a direct owner call from a constructor spelled with that name; a file that imports the owner's own type is unchanged (#13525).
