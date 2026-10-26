use std::sync::LazyLock;

use techxt::Converter;
use techxt::convert::UnknownMacroPolicy;
use techxt::mathfmt::{FontStyle, FontStyleKind};

/// Upright letters, because the italic math alphabet is missing from most terminal fonts and
/// falls back to another face. An unknown command stays as written, so a formula never loses a
/// term in silence.
static CONVERTER: LazyLock<Option<Converter>> = LazyLock::new(|| {
    Converter::builder()
        .wrap_width(None)
        .math_font(FontStyle::Style(FontStyleKind::Upright))
        .unknown_macro(UnknownMacroPolicy::KeepSource)
        .build()
        .ok()
});

/// The LaTeX of a formula as Unicode text, or the LaTeX itself when it cannot be converted.
pub fn to_unicode(latex: &str) -> String {
    CONVERTER
        .as_ref()
        .and_then(|converter| converter.latex_to_text(&format!("${latex}$")).ok())
        .map_or_else(
            || latex.to_string(),
            |conversion| conversion.text.trim().to_string(),
        )
}
