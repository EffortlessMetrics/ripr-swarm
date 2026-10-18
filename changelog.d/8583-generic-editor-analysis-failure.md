<!-- section: Fixed -->
- A failed LSP analysis is no longer silent in generic editors such as
  Helix and Neovim. Clients without the `riprEditor` integration now get
  one bounded `window/showMessage` warning per distinct failure, carrying
  the same length- and path-bounded failure text as the
  `ripr/analysisStatus` payload; an unchanged failure does not re-show on
  every refresh, and a successful analysis shows nothing. Clients with
  the `riprEditor` integration keep rendering the failure from
  `ripr/analysisStatus` and get no showMessage (#8583).
