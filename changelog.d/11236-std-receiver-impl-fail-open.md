<!-- section: Fixed -->
- A direct call to a trait default method on a std collection is no longer demoted to a name-only relation when the trait is implemented for that type by an empty impl, a qualified self type (`std::vec::Vec`), a type alias, a macro-generated impl or a multi-letter blanket impl (#11236).
