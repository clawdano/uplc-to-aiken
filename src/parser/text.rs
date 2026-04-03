use uplc::ast::{DeBruijn, Program};

use super::ParseError;

/// Parse a text-format UPLC program.
///
/// Text UPLC looks like:
/// ```text
/// (program 1.0.0
///   (lam i_0
///     (lam i_1
///       [i_0 i_1])))
/// ```
pub fn parse_text_uplc(source: &str) -> Result<Program<DeBruijn>, ParseError> {
    // Parse text UPLC into a named program, then convert to De Bruijn
    let named_program =
        uplc::parser::program(source).map_err(|e| ParseError::TextParse(format!("{}", e)))?;

    let debruijn_program = named_program
        .to_debruijn()
        .map_err(|e| ParseError::TextParse(format!("De Bruijn conversion: {}", e)))?;

    Ok(debruijn_program)
}
