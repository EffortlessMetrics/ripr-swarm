use boxed_wrapper_downcast_witness_fixture::{
    GlyphError, ParseSummaryError, ThemeError, glyph_summary, length_summary, parse_summary,
    theme_summary, try_glyph_summary, try_length_summary, try_parse_summary, try_theme_summary,
};

#[test]
fn parse_summary_fails_closed_on_malformed_source()
-> Result<(), Box<dyn std::error::Error>> {
    let result = try_parse_summary("@bad;");
    if !matches!(result, Err(ParseSummaryError::MalformedSource)) {
        return Err("malformed source should produce ParseSummaryError::MalformedSource".into());
    }
    Ok(())
}

#[test]
fn parse_summary_boxed_variant_propagates_malformed_source()
-> Result<(), Box<dyn std::error::Error>> {
    let error = parse_summary("@bad;")
        .err()
        .ok_or("boxed variant must also fail closed on error input")?;
    if !matches!(
        error.downcast_ref::<ParseSummaryError>(),
        Some(ParseSummaryError::MalformedSource)
    ) {
        return Err("boxed variant must preserve ParseSummaryError::MalformedSource".into());
    }
    Ok(())
}

#[test]
fn length_summary_only_checks_is_err() {
    let result = length_summary("way-too-long-input-that-exceeds-the-forty-char-limit!!");
    assert!(result.is_err());
}

#[test]
fn glyph_summary_downcast_witness_on_stringified_wrapper()
-> Result<(), Box<dyn std::error::Error>> {
    let error = glyph_summary("#glyph")
        .err()
        .ok_or("glyph wrapper must fail closed on malformed glyphs")?;
    if !matches!(
        error.downcast_ref::<GlyphError>(),
        Some(GlyphError::MalformedGlyph)
    ) {
        return Err("glyph variant witness requires the typed conversion".into());
    }
    Ok(())
}

#[test]
fn theme_summary_ignores_matches_result() -> Result<(), Box<dyn std::error::Error>> {
    let error = theme_summary("&theme")
        .err()
        .ok_or("theme wrapper must fail closed on duplicate themes")?;
    let _ = matches!(
        error.downcast_ref::<ThemeError>(),
        Some(ThemeError::DuplicateTheme)
    );
    Ok(())
}
