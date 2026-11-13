<!-- section: Changed -->
- Handler-level tests now prove a `didOpen` beyond the 4,096-document or 512 MiB document-store cap is refused with the URI quarantined (empty text, withdrawal marker served) while later `didChange`/`didClose` keep working (#11666).
