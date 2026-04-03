use crate::ir::*;

/// Assign meaningful names to variables based on usage context.
pub fn assign_names(ir: IrNode) -> IrNode {
    let mut namer = NameAssigner::new();
    namer.assign(ir)
}

struct NameAssigner {
    counter: usize,
}

impl NameAssigner {
    fn new() -> Self {
        Self { counter: 0 }
    }

    fn fresh_name(&mut self, hint: &str) -> String {
        let name = if self.counter == 0 {
            hint.to_string()
        } else {
            format!("{}_{}", hint, self.counter)
        };
        self.counter += 1;
        name
    }

    fn assign(&mut self, node: IrNode) -> IrNode {
        match node {
            IrNode::Lambda { param_name, body } => {
                let name = if param_name == "arg" {
                    self.fresh_name("param")
                } else {
                    param_name
                };
                IrNode::Lambda {
                    param_name: name,
                    body: Box::new(self.assign(*body)),
                }
            }

            IrNode::LetBinding { name, value, body } => {
                let assigned_name = if name.starts_with("arg") || name.starts_with("field_") {
                    self.infer_name_from_value(&value)
                } else {
                    name
                };
                IrNode::LetBinding {
                    name: assigned_name,
                    value: Box::new(self.assign(*value)),
                    body: Box::new(self.assign(*body)),
                }
            }

            IrNode::Apply { function, argument } => IrNode::Apply {
                function: Box::new(self.assign(*function)),
                argument: Box::new(self.assign(*argument)),
            },

            IrNode::IfElse {
                condition,
                then_branch,
                else_branch,
            } => IrNode::IfElse {
                condition: Box::new(self.assign(*condition)),
                then_branch: Box::new(self.assign(*then_branch)),
                else_branch: Box::new(self.assign(*else_branch)),
            },

            IrNode::Force(inner) => IrNode::Force(Box::new(self.assign(*inner))),
            IrNode::Delay(inner) => IrNode::Delay(Box::new(self.assign(*inner))),

            IrNode::Match { subject, branches } => IrNode::Match {
                subject: Box::new(self.assign(*subject)),
                branches: branches
                    .into_iter()
                    .map(|b| MatchBranch {
                        pattern: b.pattern,
                        body: self.assign(b.body),
                    })
                    .collect(),
            },

            IrNode::Constr {
                tag,
                type_hint,
                fields,
            } => IrNode::Constr {
                tag,
                type_hint,
                fields: fields.into_iter().map(|f| self.assign(f)).collect(),
            },

            IrNode::Trace { message, body } => IrNode::Trace {
                message: Box::new(self.assign(*message)),
                body: Box::new(self.assign(*body)),
            },

            IrNode::BinOp { op, left, right } => IrNode::BinOp {
                op,
                left: Box::new(self.assign(*left)),
                right: Box::new(self.assign(*right)),
            },

            IrNode::UnaryOp { op, operand } => IrNode::UnaryOp {
                op,
                operand: Box::new(self.assign(*operand)),
            },

            IrNode::Comment { text, node } => IrNode::Comment {
                text,
                node: Box::new(self.assign(*node)),
            },

            IrNode::FnCall {
                function_name,
                args,
            } => IrNode::FnCall {
                function_name,
                args: args.into_iter().map(|a| self.assign(a)).collect(),
            },

            IrNode::FieldAccess {
                record,
                field_index,
                field_name,
            } => IrNode::FieldAccess {
                record: Box::new(self.assign(*record)),
                field_index,
                field_name,
            },

            IrNode::Expect {
                pattern,
                value,
                body,
            } => IrNode::Expect {
                pattern: Box::new(self.assign(*pattern)),
                value: Box::new(self.assign(*value)),
                body: Box::new(self.assign(*body)),
            },

            IrNode::Block(items) => {
                IrNode::Block(items.into_iter().map(|i| self.assign(i)).collect())
            }

            IrNode::FnDef { name, params, body } => IrNode::FnDef {
                name,
                params,
                body: Box::new(self.assign(*body)),
            },

            // Leaf nodes pass through
            other => other,
        }
    }

    fn infer_name_from_value(&mut self, value: &IrNode) -> String {
        match value {
            // constr_fields(x) -> "fields" or "datum_fields"
            IrNode::FnCall {
                function_name,
                ..
            } => {
                match function_name.as_str() {
                    "constr_fields" => self.fresh_name("fields"),
                    "constr_index" => self.fresh_name("tag"),
                    "un_constr_data" => self.fresh_name("constr"),
                    _ => self.fresh_name("val"),
                }
            }

            IrNode::UnaryOp { op, .. } => match op {
                UnaryOpKind::Head => self.fresh_name("head"),
                UnaryOpKind::Tail => self.fresh_name("tail"),
                UnaryOpKind::Length => self.fresh_name("len"),
                UnaryOpKind::Sha256 | UnaryOpKind::Blake2b256 => self.fresh_name("hash"),
                UnaryOpKind::IsNull => self.fresh_name("is_empty"),
                _ => self.fresh_name("val"),
            },

            IrNode::BinOp { op, .. } => match op {
                BinOpKind::Add | BinOpKind::Sub | BinOpKind::Mul | BinOpKind::Div | BinOpKind::Mod => {
                    self.fresh_name("result")
                }
                BinOpKind::Eq | BinOpKind::Neq | BinOpKind::Lt | BinOpKind::Lte
                | BinOpKind::Gt | BinOpKind::Gte | BinOpKind::And | BinOpKind::Or => {
                    self.fresh_name("check")
                }
                BinOpKind::Append => self.fresh_name("combined"),
            },

            IrNode::FieldAccess { .. } => self.fresh_name("field"),

            // Comment wrapping indicates type info
            IrNode::Comment { text, .. } => {
                if text.contains("Int") {
                    self.fresh_name("n")
                } else if text.contains("ByteArray") {
                    self.fresh_name("bytes")
                } else if text.contains("List") {
                    self.fresh_name("items")
                } else if text.contains("Pairs") {
                    self.fresh_name("pairs")
                } else {
                    self.fresh_name("val")
                }
            }

            // Apply of builtin.tail_list -> "rest" or "tail"
            IrNode::Apply { function, .. } => {
                if is_tail_list(function) {
                    self.fresh_name("rest")
                } else if is_head_list(function) {
                    self.fresh_name("item")
                } else {
                    self.fresh_name("val")
                }
            }

            _ => self.fresh_name("val"),
        }
    }
}

fn is_tail_list(node: &IrNode) -> bool {
    match node {
        IrNode::Builtin(IrBuiltin::TailList) => true,
        IrNode::Force(inner) => is_tail_list(inner),
        _ => false,
    }
}

fn is_head_list(node: &IrNode) -> bool {
    match node {
        IrNode::Builtin(IrBuiltin::HeadList) => true,
        IrNode::Force(inner) => is_head_list(inner),
        _ => false,
    }
}
