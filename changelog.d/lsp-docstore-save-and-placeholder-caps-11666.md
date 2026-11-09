<!-- section: Fixed -->
- LSP: a healthy document's client-provided `didSave` text is now refused past the retained-text limit with `document_store_limit_reached` instead of being adopted, and overflow placeholders are capped at 4,096; a refused URI beyond that is not stored and reads as withdrawn (`document_store_limit_reached`) until all documents are closed (#11666).
