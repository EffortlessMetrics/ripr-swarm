<!-- section: Fixed -->

- A test file that imports the owner type's name from a crate outside the workspace (for example `use other_crate::Regex;`) no longer earns a direct owner call from a constructor spelled with that name; imports of the owner's own crate or other workspace members are unchanged (#13525).
