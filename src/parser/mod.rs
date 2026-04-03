use std::path::Path;

use uplc::ast::{DeBruijn, Program, Term};

mod cbor;
mod text;

pub use self::cbor::parse_cbor_hex;
pub use self::text::parse_text_uplc;

#[derive(Debug, thiserror::Error)]
pub enum ParseError {
    #[error("Failed to read file: {0}")]
    Io(#[from] std::io::Error),

    #[error("Failed to decode CBOR hex: {0}")]
    CborDecode(String),

    #[error("Failed to parse text UPLC: {0}")]
    TextParse(String),

    #[error("Invalid hex string: {0}")]
    InvalidHex(String),
}

impl From<ParseError> for miette::Report {
    fn from(e: ParseError) -> Self {
        miette::miette!("{}", e)
    }
}

pub type UplcTerm = Term<DeBruijn>;
pub type UplcProgram = Program<DeBruijn>;

/// Parse UPLC from a file. Auto-detects format:
/// - If the file content is valid hex, treats it as CBOR hex
/// - Otherwise, tries to parse as text-format UPLC
pub fn parse_from_file(path: &Path) -> Result<UplcProgram, ParseError> {
    let content = std::fs::read_to_string(path)?;
    let trimmed = content.trim();

    // Try as hex first (CBOR-encoded flat)
    if trimmed.chars().all(|c| c.is_ascii_hexdigit()) {
        return parse_cbor_hex(trimmed);
    }

    // Otherwise parse as text UPLC
    parse_text_uplc(trimmed)
}

/// Parse UPLC from a CBOR hex string
pub fn parse_from_cbor_hex(hex_str: &str) -> Result<UplcProgram, ParseError> {
    parse_cbor_hex(hex_str.trim())
}
