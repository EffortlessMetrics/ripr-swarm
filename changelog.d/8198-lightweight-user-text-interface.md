<!-- section: Internal / CI -->
- Add a product-independent `cargo policy check-user-text --tree` interface sharing
  the existing matcher with xtask, preserving historical base pins and live checks;
  hosted activation follows only after the trusted base contains this interface (#8198).
