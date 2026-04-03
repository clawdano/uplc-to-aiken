use uplc::ast::{DeBruijn, Program};

use super::ParseError;

/// Parse a CBOR hex-encoded UPLC program.
///
/// On-chain Cardano scripts are encoded as: CBOR wrapping of flat-encoded UPLC.
/// The `uplc` crate handles both the CBOR unwrapping and flat decoding.
pub fn parse_cbor_hex(hex_str: &str) -> Result<Program<DeBruijn>, ParseError> {
    let mut cbor_buffer = Vec::new();
    let mut flat_buffer = Vec::new();

    let program: Program<DeBruijn> =
        Program::from_hex(hex_str, &mut cbor_buffer, &mut flat_buffer)
            .map_err(|e| ParseError::CborDecode(format!("{}", e)))?;

    Ok(program)
}
