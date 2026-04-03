use num_bigint::BigInt as NumBigInt;
use num_traits::ToPrimitive;
use uplc::ast::{Constant, DeBruijn, Program, Term};
use uplc::builtins::DefaultFunction;

use super::node::*;

/// Lower a UPLC Program into our IR representation.
///
/// This is a direct, mechanical translation - no pattern recognition happens here.
/// The decompiler passes will later transform the IR into higher-level constructs.
pub fn lower(program: &Program<DeBruijn>) -> IrNode {
    lower_term(&program.term)
}

fn lower_term(term: &Term<DeBruijn>) -> IrNode {
    match term {
        Term::Var(name) => IrNode::Var(name.inner() as usize),

        Term::Lambda { body, .. } => {
            let param_name = "arg".to_string();
            IrNode::Lambda {
                param_name,
                body: Box::new(lower_term(body)),
            }
        }

        Term::Apply { function, argument } => IrNode::Apply {
            function: Box::new(lower_term(function)),
            argument: Box::new(lower_term(argument)),
        },

        Term::Constant(constant) => lower_constant(constant.as_ref()),

        Term::Builtin(builtin) => IrNode::Builtin(lower_builtin(*builtin)),

        Term::Force(inner) => IrNode::Force(Box::new(lower_term(inner))),

        Term::Delay(inner) => IrNode::Delay(Box::new(lower_term(inner))),

        Term::Error => IrNode::Error,

        Term::Constr { tag, fields } => IrNode::Constr {
            tag: *tag,
            type_hint: None,
            fields: fields.iter().map(|f| lower_term(f)).collect(),
        },

        Term::Case { constr, branches } => {
            let ir_branches = branches
                .iter()
                .enumerate()
                .map(|(i, branch)| MatchBranch {
                    pattern: MatchPattern::Constructor {
                        tag: i,
                        type_hint: None,
                        bindings: Vec::new(),
                    },
                    body: lower_term(branch),
                })
                .collect();

            IrNode::Match {
                subject: Box::new(lower_term(constr)),
                branches: ir_branches,
            }
        }
    }
}

fn bigint_to_i128(n: &NumBigInt) -> i128 {
    n.to_i128().unwrap_or(0)
}

fn lower_constant(constant: &Constant) -> IrNode {
    match constant {
        Constant::Integer(n) => IrNode::Constant(IrConstant::Integer(bigint_to_i128(n))),
        Constant::ByteString(bs) => IrNode::Constant(IrConstant::ByteString(bs.clone())),
        Constant::String(s) => IrNode::Constant(IrConstant::String(s.clone())),
        Constant::Bool(b) => IrNode::Constant(IrConstant::Bool(*b)),
        Constant::Unit => IrNode::Constant(IrConstant::Unit),
        Constant::Data(data) => IrNode::Constant(IrConstant::Data(lower_data(data))),
        Constant::ProtoList(_, items) => {
            let ir_items = items.iter().map(|c| lower_constant_value(c)).collect();
            IrNode::Constant(IrConstant::List(ir_items))
        }
        Constant::ProtoPair(_, _, left, right) => IrNode::Constant(IrConstant::Pair(
            Box::new(lower_constant_value(left)),
            Box::new(lower_constant_value(right)),
        )),
        _ => IrNode::Constant(IrConstant::Unit), // BLS types etc - fallback
    }
}

fn lower_constant_value(constant: &Constant) -> IrConstant {
    match constant {
        Constant::Integer(n) => IrConstant::Integer(bigint_to_i128(n)),
        Constant::ByteString(bs) => IrConstant::ByteString(bs.clone()),
        Constant::String(s) => IrConstant::String(s.clone()),
        Constant::Bool(b) => IrConstant::Bool(*b),
        Constant::Unit => IrConstant::Unit,
        Constant::Data(data) => IrConstant::Data(lower_data(data)),
        Constant::ProtoList(_, items) => {
            IrConstant::List(items.iter().map(|c| lower_constant_value(c)).collect())
        }
        Constant::ProtoPair(_, _, left, right) => IrConstant::Pair(
            Box::new(lower_constant_value(left)),
            Box::new(lower_constant_value(right)),
        ),
        _ => IrConstant::Unit,
    }
}

fn lower_data(data: &uplc::PlutusData) -> IrData {
    match data {
        uplc::PlutusData::Constr(constr) => {
            let fields = constr.fields.iter().map(|f| lower_data(f)).collect();
            IrData::Constr(constr.tag, fields)
        }
        uplc::PlutusData::Map(pairs) => {
            let ir_pairs = pairs
                .iter()
                .map(|p| (lower_data(&p.0), lower_data(&p.1)))
                .collect();
            IrData::Map(ir_pairs)
        }
        uplc::PlutusData::BigInt(n) => {
            let val: i128 = match n {
                uplc::BigInt::Int(i) => (*i).into(),
                uplc::BigInt::BigUInt(bytes) => {
                    // Positive big integer from bytes
                    let mut result: i128 = 0;
                    for b in bytes.iter() {
                        result = (result << 8) | (*b as i128);
                    }
                    result
                }
                uplc::BigInt::BigNInt(bytes) => {
                    // Negative big integer from bytes
                    let mut result: i128 = 0;
                    for b in bytes.iter() {
                        result = (result << 8) | (*b as i128);
                    }
                    -result
                }
            };
            IrData::Integer(val)
        }
        uplc::PlutusData::BoundedBytes(bs) => IrData::ByteString(bs.to_vec()),
        uplc::PlutusData::Array(items) => {
            IrData::List(items.iter().map(|d| lower_data(d)).collect())
        }
    }
}

fn lower_builtin(builtin: DefaultFunction) -> IrBuiltin {
    match builtin {
        DefaultFunction::AddInteger => IrBuiltin::AddInteger,
        DefaultFunction::SubtractInteger => IrBuiltin::SubtractInteger,
        DefaultFunction::MultiplyInteger => IrBuiltin::MultiplyInteger,
        DefaultFunction::DivideInteger => IrBuiltin::DivideInteger,
        DefaultFunction::ModInteger => IrBuiltin::ModInteger,
        DefaultFunction::QuotientInteger => IrBuiltin::QuotientInteger,
        DefaultFunction::RemainderInteger => IrBuiltin::RemainderInteger,
        DefaultFunction::EqualsInteger => IrBuiltin::EqualsInteger,
        DefaultFunction::LessThanInteger => IrBuiltin::LessThanInteger,
        DefaultFunction::LessThanEqualsInteger => IrBuiltin::LessThanEqualsInteger,
        DefaultFunction::AppendByteString => IrBuiltin::AppendByteString,
        DefaultFunction::ConsByteString => IrBuiltin::ConsByteString,
        DefaultFunction::SliceByteString => IrBuiltin::SliceByteString,
        DefaultFunction::LengthOfByteString => IrBuiltin::LengthOfByteString,
        DefaultFunction::IndexByteString => IrBuiltin::IndexByteString,
        DefaultFunction::EqualsByteString => IrBuiltin::EqualsByteString,
        DefaultFunction::LessThanByteString => IrBuiltin::LessThanByteString,
        DefaultFunction::LessThanEqualsByteString => IrBuiltin::LessThanEqualsByteString,
        DefaultFunction::Sha2_256 => IrBuiltin::Sha2_256,
        DefaultFunction::Sha3_256 => IrBuiltin::Sha3_256,
        DefaultFunction::Blake2b_256 => IrBuiltin::Blake2b_256,
        DefaultFunction::VerifyEd25519Signature => IrBuiltin::VerifyEd25519Signature,
        DefaultFunction::AppendString => IrBuiltin::AppendString,
        DefaultFunction::EqualsString => IrBuiltin::EqualsString,
        DefaultFunction::EncodeUtf8 => IrBuiltin::EncodeUtf8,
        DefaultFunction::DecodeUtf8 => IrBuiltin::DecodeUtf8,
        DefaultFunction::IfThenElse => IrBuiltin::IfThenElse,
        DefaultFunction::ChooseUnit => IrBuiltin::ChooseUnit,
        DefaultFunction::Trace => IrBuiltin::Trace,
        DefaultFunction::FstPair => IrBuiltin::FstPair,
        DefaultFunction::SndPair => IrBuiltin::SndPair,
        DefaultFunction::ChooseList => IrBuiltin::ChooseList,
        DefaultFunction::MkCons => IrBuiltin::MkCons,
        DefaultFunction::HeadList => IrBuiltin::HeadList,
        DefaultFunction::TailList => IrBuiltin::TailList,
        DefaultFunction::NullList => IrBuiltin::NullList,
        DefaultFunction::MkNilData => IrBuiltin::MkNilData,
        DefaultFunction::MkNilPairData => IrBuiltin::MkNilPairData,
        DefaultFunction::ConstrData => IrBuiltin::ConstrData,
        DefaultFunction::MapData => IrBuiltin::MapData,
        DefaultFunction::ListData => IrBuiltin::ListData,
        DefaultFunction::IData => IrBuiltin::IData,
        DefaultFunction::BData => IrBuiltin::BData,
        DefaultFunction::UnConstrData => IrBuiltin::UnConstrData,
        DefaultFunction::UnMapData => IrBuiltin::UnMapData,
        DefaultFunction::UnListData => IrBuiltin::UnListData,
        DefaultFunction::UnIData => IrBuiltin::UnIData,
        DefaultFunction::UnBData => IrBuiltin::UnBData,
        DefaultFunction::EqualsData => IrBuiltin::EqualsData,
        DefaultFunction::SerialiseData => IrBuiltin::SerialiseData,
        DefaultFunction::ChooseData => IrBuiltin::ChooseData,
        DefaultFunction::MkPairData => IrBuiltin::MkPairData,
        other => IrBuiltin::Other(format!("{:?}", other)),
    }
}
