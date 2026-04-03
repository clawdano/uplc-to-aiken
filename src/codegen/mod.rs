use crate::ir::*;

/// Emit Aiken source code from the decompiled IR.
pub fn emit(node: &IrNode) -> String {
    let mut emitter = AikenEmitter::new();
    emitter.emit_node(node, 0);
    emitter.output
}

struct AikenEmitter {
    output: String,
    /// Scope stack for resolving De Bruijn indices to names.
    /// Index 0 = outermost binding.
    scope: Vec<String>,
}

impl AikenEmitter {
    fn new() -> Self {
        Self {
            output: String::new(),
            scope: Vec::new(),
        }
    }

    fn push_scope(&mut self, name: &str) {
        self.scope.push(name.to_string());
    }

    fn pop_scope(&mut self) {
        self.scope.pop();
    }

    fn resolve_var(&self, index: usize) -> String {
        if index > 0 && index <= self.scope.len() {
            self.scope[self.scope.len() - index].clone()
        } else {
            format!("var_{}", index)
        }
    }

    fn indent(&mut self, level: usize) {
        for _ in 0..level {
            self.output.push_str("  ");
        }
    }

    fn emit_node(&mut self, node: &IrNode, indent: usize) {
        match node {
            IrNode::Var(idx) => {
                self.output.push_str(&self.resolve_var(*idx));
            }

            IrNode::Lambda { param_name, body } => {
                self.output.push_str(&format!("fn({}) {{\n", param_name));
                self.push_scope(param_name);
                self.indent(indent + 1);
                self.emit_node(body, indent + 1);
                self.output.push('\n');
                self.indent(indent);
                self.output.push('}');
                self.pop_scope();
            }

            IrNode::Apply {
                function,
                argument,
            } => {
                self.emit_node(function, indent);
                self.output.push('(');
                self.emit_node(argument, indent);
                self.output.push(')');
            }

            IrNode::Constant(c) => self.emit_constant(c),

            IrNode::Builtin(b) => {
                self.output.push_str(&builtin_name(b));
            }

            IrNode::Force(inner) => {
                // Force is a UPLC implementation detail, try to elide it
                self.emit_node(inner, indent);
            }

            IrNode::Delay(inner) => {
                // Delay is a UPLC implementation detail, try to elide it
                self.emit_node(inner, indent);
            }

            IrNode::Error => {
                self.output.push_str("fail");
            }

            IrNode::IfElse {
                condition,
                then_branch,
                else_branch,
            } => {
                self.output.push_str("if ");
                self.emit_node(condition, indent);
                self.output.push_str(" {\n");
                self.indent(indent + 1);
                self.emit_node(then_branch, indent + 1);
                self.output.push('\n');
                self.indent(indent);
                self.output.push_str("} else {\n");
                self.indent(indent + 1);
                self.emit_node(else_branch, indent + 1);
                self.output.push('\n');
                self.indent(indent);
                self.output.push('}');
            }

            IrNode::LetBinding { name, value, body } => {
                self.output.push_str(&format!("let {} = ", name));
                self.emit_node(value, indent);
                self.output.push('\n');
                self.push_scope(name);
                self.indent(indent);
                self.emit_node(body, indent);
                self.pop_scope();
            }

            IrNode::BinOp { op, left, right } => {
                let needs_parens = matches!(
                    left.as_ref(),
                    IrNode::BinOp { .. } | IrNode::IfElse { .. }
                );
                if needs_parens {
                    self.output.push('(');
                }
                self.emit_node(left, indent);
                if needs_parens {
                    self.output.push(')');
                }
                self.output.push_str(&format!(" {} ", op));
                let needs_parens = matches!(
                    right.as_ref(),
                    IrNode::BinOp { .. } | IrNode::IfElse { .. }
                );
                if needs_parens {
                    self.output.push('(');
                }
                self.emit_node(right, indent);
                if needs_parens {
                    self.output.push(')');
                }
            }

            IrNode::UnaryOp { op, operand } => {
                match op {
                    UnaryOpKind::Negate => {
                        self.output.push('-');
                        self.emit_node(operand, indent);
                    }
                    UnaryOpKind::Not => {
                        self.output.push('!');
                        self.emit_node(operand, indent);
                    }
                    UnaryOpKind::Length => {
                        self.output.push_str("builtin.length_of_bytearray(");
                        self.emit_node(operand, indent);
                        self.output.push(')');
                    }
                    UnaryOpKind::Head => {
                        self.output.push_str("builtin.head_list(");
                        self.emit_node(operand, indent);
                        self.output.push(')');
                    }
                    UnaryOpKind::Tail => {
                        self.output.push_str("builtin.tail_list(");
                        self.emit_node(operand, indent);
                        self.output.push(')');
                    }
                    UnaryOpKind::IsNull => {
                        self.output.push_str("list.is_empty(");
                        self.emit_node(operand, indent);
                        self.output.push(')');
                    }
                    UnaryOpKind::Sha256 => {
                        self.output.push_str("crypto.sha2_256(");
                        self.emit_node(operand, indent);
                        self.output.push(')');
                    }
                    UnaryOpKind::Blake2b256 => {
                        self.output.push_str("crypto.blake2b_256(");
                        self.emit_node(operand, indent);
                        self.output.push(')');
                    }
                    UnaryOpKind::EncodeUtf8 => {
                        self.output.push_str("string.to_bytearray(");
                        self.emit_node(operand, indent);
                        self.output.push(')');
                    }
                    UnaryOpKind::DecodeUtf8 => {
                        self.output.push_str("bytearray.to_string(");
                        self.emit_node(operand, indent);
                        self.output.push(')');
                    }
                }
            }

            IrNode::Match { subject, branches } => {
                self.output.push_str("when ");
                self.emit_node(subject, indent);
                self.output.push_str(" is {\n");
                for branch in branches {
                    self.indent(indent + 1);
                    self.emit_match_pattern(&branch.pattern);
                    self.output.push_str(" -> ");
                    self.emit_node(&branch.body, indent + 1);
                    self.output.push('\n');
                }
                self.indent(indent);
                self.output.push('}');
            }

            IrNode::Constr {
                tag,
                type_hint,
                fields,
            } => {
                if let Some(hint) = type_hint {
                    self.output.push_str(hint);
                } else {
                    self.output.push_str(&format!("Constr_{}", tag));
                }
                if !fields.is_empty() {
                    self.output.push('(');
                    for (i, field) in fields.iter().enumerate() {
                        if i > 0 {
                            self.output.push_str(", ");
                        }
                        self.emit_node(field, indent);
                    }
                    self.output.push(')');
                }
            }

            IrNode::FieldAccess {
                record,
                field_index,
                field_name,
            } => {
                self.emit_node(record, indent);
                self.output.push('.');
                if let Some(name) = field_name {
                    self.output.push_str(name);
                } else {
                    self.output.push_str(&format!("field_{}", field_index));
                }
            }

            IrNode::FnDef {
                name,
                params,
                body,
            } => {
                self.output
                    .push_str(&format!("fn {}({}) {{\n", name, params.join(", ")));
                self.indent(indent + 1);
                self.emit_node(body, indent + 1);
                self.output.push('\n');
                self.indent(indent);
                self.output.push('}');
            }

            IrNode::Validator {
                name,
                params,
                body,
            } => {
                self.output.push_str(&format!("validator {} {{\n", name));
                self.indent(indent + 1);
                let param_strs: Vec<String> = params
                    .iter()
                    .map(|p| match &p.type_hint {
                        Some(t) => format!("{}: {}", p.name, t),
                        None => p.name.clone(),
                    })
                    .collect();
                self.output
                    .push_str(&format!("fn validate({}) {{\n", param_strs.join(", ")));
                self.indent(indent + 2);
                self.emit_node(body, indent + 2);
                self.output.push('\n');
                self.indent(indent + 1);
                self.output.push_str("}\n");
                self.indent(indent);
                self.output.push('}');
            }

            IrNode::Expect {
                pattern,
                value,
                body,
            } => {
                self.output.push_str("expect ");
                self.emit_node(pattern, indent);
                self.output.push_str(" = ");
                self.emit_node(value, indent);
                self.output.push('\n');
                self.indent(indent);
                self.emit_node(body, indent);
            }

            IrNode::Trace { message, body } => {
                self.output.push_str("trace ");
                self.emit_node(message, indent);
                self.output.push('\n');
                self.indent(indent);
                self.emit_node(body, indent);
            }

            IrNode::FnCall {
                function_name,
                args,
            } => {
                self.output.push_str(function_name);
                self.output.push('(');
                for (i, arg) in args.iter().enumerate() {
                    if i > 0 {
                        self.output.push_str(", ");
                    }
                    self.emit_node(arg, indent);
                }
                self.output.push(')');
            }

            IrNode::ListLit(items) => {
                self.output.push('[');
                for (i, item) in items.iter().enumerate() {
                    if i > 0 {
                        self.output.push_str(", ");
                    }
                    self.emit_node(item, indent);
                }
                self.output.push(']');
            }

            IrNode::TupleLit(items) => {
                self.output.push('(');
                for (i, item) in items.iter().enumerate() {
                    if i > 0 {
                        self.output.push_str(", ");
                    }
                    self.emit_node(item, indent);
                }
                self.output.push(')');
            }

            IrNode::ByteArrayLit(bytes) => {
                self.output.push_str(&format!("#\"{}\"", hex::encode(bytes)));
            }

            IrNode::StringLit(s) => {
                self.output.push_str(&format!("@\"{}\"", s));
            }

            IrNode::IntLit(n) => {
                self.output.push_str(&n.to_string());
            }

            IrNode::BoolLit(b) => {
                self.output.push_str(if *b { "True" } else { "False" });
            }

            IrNode::Unit => {
                self.output.push_str("Void");
            }

            IrNode::Block(items) => {
                for (i, item) in items.iter().enumerate() {
                    if i > 0 {
                        self.output.push('\n');
                        self.indent(indent);
                    }
                    self.emit_node(item, indent);
                }
            }

            IrNode::Comment { text, node } => {
                self.output.push_str(&format!("// {}\n", text));
                self.indent(indent);
                self.emit_node(node, indent);
            }
        }
    }

    fn emit_constant(&mut self, constant: &IrConstant) {
        match constant {
            IrConstant::Integer(n) => self.output.push_str(&n.to_string()),
            IrConstant::ByteString(bs) => {
                self.output.push_str(&format!("#\"{}\"", hex::encode(bs)));
            }
            IrConstant::String(s) => {
                self.output.push_str(&format!("@\"{}\"", s));
            }
            IrConstant::Bool(b) => {
                self.output.push_str(if *b { "True" } else { "False" });
            }
            IrConstant::Unit => self.output.push_str("Void"),
            IrConstant::Data(data) => self.emit_data(data),
            IrConstant::List(items) => {
                self.output.push('[');
                for (i, item) in items.iter().enumerate() {
                    if i > 0 {
                        self.output.push_str(", ");
                    }
                    self.emit_constant(item);
                }
                self.output.push(']');
            }
            IrConstant::Pair(left, right) => {
                self.output.push_str("Pair(");
                self.emit_constant(left);
                self.output.push_str(", ");
                self.emit_constant(right);
                self.output.push(')');
            }
        }
    }

    fn emit_data(&mut self, data: &IrData) {
        match data {
            IrData::Constr(tag, fields) => {
                self.output.push_str(&format!("Constr({}, [", tag));
                for (i, field) in fields.iter().enumerate() {
                    if i > 0 {
                        self.output.push_str(", ");
                    }
                    self.emit_data(field);
                }
                self.output.push_str("])");
            }
            IrData::Map(pairs) => {
                self.output.push_str("Map([");
                for (i, (k, v)) in pairs.iter().enumerate() {
                    if i > 0 {
                        self.output.push_str(", ");
                    }
                    self.output.push_str("Pair(");
                    self.emit_data(k);
                    self.output.push_str(", ");
                    self.emit_data(v);
                    self.output.push(')');
                }
                self.output.push_str("])");
            }
            IrData::List(items) => {
                self.output.push('[');
                for (i, item) in items.iter().enumerate() {
                    if i > 0 {
                        self.output.push_str(", ");
                    }
                    self.emit_data(item);
                }
                self.output.push(']');
            }
            IrData::Integer(n) => self.output.push_str(&n.to_string()),
            IrData::ByteString(bs) => {
                self.output.push_str(&format!("#\"{}\"", hex::encode(bs)));
            }
        }
    }

    fn emit_match_pattern(&mut self, pattern: &MatchPattern) {
        match pattern {
            MatchPattern::Constructor {
                tag,
                type_hint,
                bindings,
            } => {
                if let Some(hint) = type_hint {
                    self.output.push_str(hint);
                } else {
                    self.output.push_str(&format!("Constr_{}", tag));
                }
                if !bindings.is_empty() {
                    self.output.push('(');
                    self.output.push_str(&bindings.join(", "));
                    self.output.push(')');
                }
            }
            MatchPattern::Wildcard => {
                self.output.push('_');
            }
        }
    }
}

fn builtin_name(builtin: &IrBuiltin) -> String {
    match builtin {
        IrBuiltin::AddInteger => "builtin.add_integer".to_string(),
        IrBuiltin::SubtractInteger => "builtin.subtract_integer".to_string(),
        IrBuiltin::MultiplyInteger => "builtin.multiply_integer".to_string(),
        IrBuiltin::DivideInteger => "builtin.divide_integer".to_string(),
        IrBuiltin::ModInteger => "builtin.mod_integer".to_string(),
        IrBuiltin::QuotientInteger => "builtin.quotient_integer".to_string(),
        IrBuiltin::RemainderInteger => "builtin.remainder_integer".to_string(),
        IrBuiltin::EqualsInteger => "builtin.equals_integer".to_string(),
        IrBuiltin::LessThanInteger => "builtin.less_than_integer".to_string(),
        IrBuiltin::LessThanEqualsInteger => "builtin.less_than_equals_integer".to_string(),
        IrBuiltin::AppendByteString => "builtin.append_bytearray".to_string(),
        IrBuiltin::ConsByteString => "builtin.cons_bytearray".to_string(),
        IrBuiltin::SliceByteString => "builtin.slice_bytearray".to_string(),
        IrBuiltin::LengthOfByteString => "builtin.length_of_bytearray".to_string(),
        IrBuiltin::IndexByteString => "builtin.index_bytearray".to_string(),
        IrBuiltin::EqualsByteString => "builtin.equals_bytearray".to_string(),
        IrBuiltin::LessThanByteString => "builtin.less_than_bytearray".to_string(),
        IrBuiltin::LessThanEqualsByteString => "builtin.less_than_equals_bytearray".to_string(),
        IrBuiltin::Sha2_256 => "crypto.sha2_256".to_string(),
        IrBuiltin::Sha3_256 => "crypto.sha3_256".to_string(),
        IrBuiltin::Blake2b_256 => "crypto.blake2b_256".to_string(),
        IrBuiltin::VerifyEd25519Signature => "crypto.verify_ed25519_signature".to_string(),
        IrBuiltin::AppendString => "string.concat".to_string(),
        IrBuiltin::EqualsString => "string.equals".to_string(),
        IrBuiltin::EncodeUtf8 => "string.to_bytearray".to_string(),
        IrBuiltin::DecodeUtf8 => "bytearray.to_string".to_string(),
        IrBuiltin::IfThenElse => "builtin.if_then_else".to_string(),
        IrBuiltin::ChooseUnit => "builtin.choose_unit".to_string(),
        IrBuiltin::Trace => "trace".to_string(),
        IrBuiltin::FstPair => "builtin.fst_pair".to_string(),
        IrBuiltin::SndPair => "builtin.snd_pair".to_string(),
        IrBuiltin::ChooseList => "builtin.choose_list".to_string(),
        IrBuiltin::MkCons => "builtin.cons_list".to_string(),
        IrBuiltin::HeadList => "builtin.head_list".to_string(),
        IrBuiltin::TailList => "builtin.tail_list".to_string(),
        IrBuiltin::NullList => "builtin.null_list".to_string(),
        IrBuiltin::MkNilData => "builtin.mk_nil_data".to_string(),
        IrBuiltin::MkNilPairData => "builtin.mk_nil_pair_data".to_string(),
        IrBuiltin::ConstrData => "builtin.constr_data".to_string(),
        IrBuiltin::MapData => "builtin.map_data".to_string(),
        IrBuiltin::ListData => "builtin.list_data".to_string(),
        IrBuiltin::IData => "builtin.i_data".to_string(),
        IrBuiltin::BData => "builtin.b_data".to_string(),
        IrBuiltin::UnConstrData => "builtin.un_constr_data".to_string(),
        IrBuiltin::UnMapData => "builtin.un_map_data".to_string(),
        IrBuiltin::UnListData => "builtin.un_list_data".to_string(),
        IrBuiltin::UnIData => "builtin.un_i_data".to_string(),
        IrBuiltin::UnBData => "builtin.un_b_data".to_string(),
        IrBuiltin::EqualsData => "builtin.equals_data".to_string(),
        IrBuiltin::SerialiseData => "builtin.serialise_data".to_string(),
        IrBuiltin::ChooseData => "builtin.choose_data".to_string(),
        IrBuiltin::MkPairData => "builtin.mk_pair_data".to_string(),
        IrBuiltin::Other(name) => format!("builtin.{}", name.to_lowercase()),
    }
}
