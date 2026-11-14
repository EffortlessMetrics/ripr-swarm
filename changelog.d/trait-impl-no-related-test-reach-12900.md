<!-- section: Fixed -->
- A changed method of a foreign trait (`Display::fmt`, `Add::add`) that no test is related to no longer reads `no_static_path`: formatting, operators and generic helpers run it without spelling its name, so its reach is now undecided (`infection_unknown`). Plain functions and workspace-trait methods are unchanged (#12900).
