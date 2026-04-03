use crate::ir::IrNode;

/// Assign meaningful names to variables based on usage context.
///
/// UPLC uses De Bruijn indices, so all variable names are lost during compilation.
/// This pass tries to infer reasonable names based on how variables are used:
/// - Validator params get datum/redeemer/context
/// - Variables used with UnIData get int-like names
/// - Variables used with UnBData get bytes-like names
/// - etc.
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
                let assigned_name = if name == "arg" {
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
                    .map(|b| crate::ir::MatchBranch {
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

            // Leaf nodes pass through
            other => other,
        }
    }

    fn infer_name_from_value(&mut self, value: &IrNode) -> String {
        match value {
            IrNode::UnaryOp { op, .. } => match op {
                crate::ir::UnaryOpKind::Head => self.fresh_name("head"),
                crate::ir::UnaryOpKind::Tail => self.fresh_name("tail"),
                crate::ir::UnaryOpKind::Length => self.fresh_name("len"),
                crate::ir::UnaryOpKind::Sha256 => self.fresh_name("hash"),
                crate::ir::UnaryOpKind::Blake2b256 => self.fresh_name("hash"),
                _ => self.fresh_name("val"),
            },
            IrNode::BinOp { op, .. } => match op {
                crate::ir::BinOpKind::Add
                | crate::ir::BinOpKind::Sub
                | crate::ir::BinOpKind::Mul
                | crate::ir::BinOpKind::Div => self.fresh_name("result"),
                crate::ir::BinOpKind::Eq
                | crate::ir::BinOpKind::Neq
                | crate::ir::BinOpKind::Lt
                | crate::ir::BinOpKind::Lte
                | crate::ir::BinOpKind::Gt
                | crate::ir::BinOpKind::Gte => self.fresh_name("is"),
                _ => self.fresh_name("val"),
            },
            _ => self.fresh_name("val"),
        }
    }
}
