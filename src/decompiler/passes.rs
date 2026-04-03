use crate::ir::*;

/// Recognize `force (force ifThenElse condition then else)` -> IfElse node
///
/// In UPLC, if-then-else is:
///   [[[force (force (builtin ifThenElse))] condition] (delay then_branch)] (delay else_branch)
///
/// Which in De Bruijn is a series of applications of the forced IfThenElse builtin.
pub fn recognize_if_then_else(node: IrNode) -> IrNode {
    match node {
        // Pattern: Apply(Apply(Apply(Force(Force(Builtin(IfThenElse))), cond), Delay(then)), Delay(else))
        IrNode::Apply {
            function: f1,
            argument: else_branch,
        } => {
            let else_branch = recognize_if_then_else(*else_branch);

            if let IrNode::Apply {
                function: f2,
                argument: then_branch,
            } = *f1
            {
                let then_branch = recognize_if_then_else(*then_branch);

                if let IrNode::Apply {
                    function: f3,
                    argument: condition,
                } = *f2
                {
                    let condition = recognize_if_then_else(*condition);

                    if is_forced_builtin(&f3, &IrBuiltin::IfThenElse) {
                        // Unwrap Delay wrappers if present
                        let then_body = unwrap_delay(then_branch);
                        let else_body = unwrap_delay(else_branch);

                        return IrNode::IfElse {
                            condition: Box::new(condition),
                            then_branch: Box::new(then_body),
                            else_branch: Box::new(else_body),
                        };
                    }

                    // Not an if-then-else, reconstruct
                    return IrNode::Apply {
                        function: Box::new(IrNode::Apply {
                            function: Box::new(IrNode::Apply {
                                function: f3,
                                argument: Box::new(condition),
                            }),
                            argument: Box::new(then_branch),
                        }),
                        argument: Box::new(else_body_fallback(else_branch)),
                    };
                }

                // Only two applications, recurse
                return IrNode::Apply {
                    function: Box::new(IrNode::Apply {
                        function: Box::new(recognize_if_then_else(*f2.clone())),
                        argument: Box::new(then_branch),
                    }),
                    argument: Box::new(else_branch),
                };
            }

            IrNode::Apply {
                function: Box::new(recognize_if_then_else(*f1.clone())),
                argument: Box::new(else_branch),
            }
        }

        _ => map_children(node, recognize_if_then_else),
    }
}

/// Recognize binary operations from builtin applications.
///
/// Pattern: Apply(Apply(Force?(Builtin(op)), left), right)
pub fn recognize_binops(node: IrNode) -> IrNode {
    match node {
        IrNode::Apply {
            function: f1,
            argument: right,
        } => {
            let right = recognize_binops(*right);

            if let IrNode::Apply {
                function: f2,
                argument: left,
            } = *f1
            {
                let left = recognize_binops(*left);

                if let Some(op) = try_extract_binop(&f2) {
                    return IrNode::BinOp {
                        op,
                        left: Box::new(left),
                        right: Box::new(right),
                    };
                }

                return IrNode::Apply {
                    function: Box::new(IrNode::Apply {
                        function: Box::new(recognize_binops(*f2)),
                        argument: Box::new(left),
                    }),
                    argument: Box::new(right),
                };
            }

            IrNode::Apply {
                function: Box::new(recognize_binops(*f1.clone())),
                argument: Box::new(right),
            }
        }

        _ => map_children(node, recognize_binops),
    }
}

/// Recognize `(\x -> body) value` as `let x = value in body`
pub fn recognize_let_bindings(node: IrNode) -> IrNode {
    match node {
        IrNode::Apply {
            function,
            argument,
        } => {
            let argument = recognize_let_bindings(*argument);

            if let IrNode::Lambda { param_name, body } = *function {
                let body = recognize_let_bindings(*body);
                return IrNode::LetBinding {
                    name: param_name,
                    value: Box::new(argument),
                    body: Box::new(body),
                };
            }

            IrNode::Apply {
                function: Box::new(recognize_let_bindings(*function.clone())),
                argument: Box::new(argument),
            }
        }

        _ => map_children(node, recognize_let_bindings),
    }
}

/// Recognize trace builtin pattern: Apply(Apply(Force(Builtin(Trace)), msg), body)
pub fn recognize_trace(node: IrNode) -> IrNode {
    match node {
        IrNode::Apply {
            function: f1,
            argument: body,
        } => {
            let body = recognize_trace(*body);

            if let IrNode::Apply {
                function: f2,
                argument: message,
            } = *f1
            {
                let message = recognize_trace(*message);

                if is_forced_builtin(&f2, &IrBuiltin::Trace) {
                    return IrNode::Trace {
                        message: Box::new(message),
                        body: Box::new(body),
                    };
                }

                return IrNode::Apply {
                    function: Box::new(IrNode::Apply {
                        function: Box::new(recognize_trace(*f2)),
                        argument: Box::new(message),
                    }),
                    argument: Box::new(body),
                };
            }

            IrNode::Apply {
                function: Box::new(recognize_trace(*f1.clone())),
                argument: Box::new(body),
            }
        }

        _ => map_children(node, recognize_trace),
    }
}

/// Recognize Bool constants: Constr(0, []) = False, Constr(1, []) = True
pub fn recognize_bool_literals(node: IrNode) -> IrNode {
    match node {
        IrNode::Constr {
            tag,
            fields,
            type_hint: _,
        } if fields.is_empty() => match tag {
            0 => IrNode::BoolLit(false),
            1 => IrNode::BoolLit(true),
            _ => IrNode::Constr {
                tag,
                type_hint: None,
                fields,
            },
        },

        IrNode::Constant(IrConstant::Bool(b)) => IrNode::BoolLit(b),

        _ => map_children(node, recognize_bool_literals),
    }
}

/// Recognize Unit: Constant(Unit) -> Unit
pub fn recognize_unit(node: IrNode) -> IrNode {
    match node {
        IrNode::Constant(IrConstant::Unit) => IrNode::Unit,
        _ => map_children(node, recognize_unit),
    }
}

/// Recognize list operations from builtin patterns
pub fn recognize_list_ops(node: IrNode) -> IrNode {
    match node {
        // HeadList: Apply(Force(Builtin(HeadList)), list)
        IrNode::Apply {
            function,
            argument,
        } => {
            let argument = recognize_list_ops(*argument);

            if is_forced_builtin(&function, &IrBuiltin::HeadList) {
                return IrNode::UnaryOp {
                    op: UnaryOpKind::Head,
                    operand: Box::new(argument),
                };
            }
            if is_forced_builtin(&function, &IrBuiltin::TailList) {
                return IrNode::UnaryOp {
                    op: UnaryOpKind::Tail,
                    operand: Box::new(argument),
                };
            }
            if is_forced_builtin(&function, &IrBuiltin::NullList) {
                return IrNode::UnaryOp {
                    op: UnaryOpKind::IsNull,
                    operand: Box::new(argument),
                };
            }

            IrNode::Apply {
                function: Box::new(recognize_list_ops(*function)),
                argument: Box::new(argument),
            }
        }
        _ => map_children(node, recognize_list_ops),
    }
}

/// Recognize Data deconstruction patterns (UnIData, UnBData, etc.)
pub fn recognize_data_deconstruction(node: IrNode) -> IrNode {
    match node {
        IrNode::Apply {
            function,
            argument,
        } => {
            let argument = recognize_data_deconstruction(*argument);

            // UnIData(x) -> a comment noting this extracts an integer from Data
            if matches!(&*function, IrNode::Builtin(IrBuiltin::UnIData)) {
                return IrNode::Comment {
                    text: "expect: Int".to_string(),
                    node: Box::new(argument),
                };
            }
            if matches!(&*function, IrNode::Builtin(IrBuiltin::UnBData)) {
                return IrNode::Comment {
                    text: "expect: ByteArray".to_string(),
                    node: Box::new(argument),
                };
            }
            if matches!(&*function, IrNode::Builtin(IrBuiltin::UnListData)) {
                return IrNode::Comment {
                    text: "expect: List<Data>".to_string(),
                    node: Box::new(argument),
                };
            }
            if matches!(&*function, IrNode::Builtin(IrBuiltin::UnMapData)) {
                return IrNode::Comment {
                    text: "expect: Pairs<Data, Data>".to_string(),
                    node: Box::new(argument),
                };
            }

            IrNode::Apply {
                function: Box::new(recognize_data_deconstruction(*function)),
                argument: Box::new(argument),
            }
        }
        _ => map_children(node, recognize_data_deconstruction),
    }
}

// === Helpers ===

fn is_forced_builtin(node: &IrNode, expected: &IrBuiltin) -> bool {
    match node {
        IrNode::Builtin(b) => std::mem::discriminant(b) == std::mem::discriminant(expected),
        IrNode::Force(inner) => is_forced_builtin(inner, expected),
        _ => false,
    }
}

fn unwrap_delay(node: IrNode) -> IrNode {
    match node {
        IrNode::Delay(inner) => *inner,
        other => other,
    }
}

fn else_body_fallback(node: IrNode) -> IrNode {
    node
}

fn try_extract_binop(node: &IrNode) -> Option<BinOpKind> {
    let builtin = extract_builtin(node)?;
    match builtin {
        IrBuiltin::AddInteger => Some(BinOpKind::Add),
        IrBuiltin::SubtractInteger => Some(BinOpKind::Sub),
        IrBuiltin::MultiplyInteger => Some(BinOpKind::Mul),
        IrBuiltin::DivideInteger => Some(BinOpKind::Div),
        IrBuiltin::ModInteger => Some(BinOpKind::Mod),
        IrBuiltin::EqualsInteger => Some(BinOpKind::Eq),
        IrBuiltin::LessThanInteger => Some(BinOpKind::Lt),
        IrBuiltin::LessThanEqualsInteger => Some(BinOpKind::Lte),
        IrBuiltin::EqualsByteString => Some(BinOpKind::Eq),
        IrBuiltin::LessThanByteString => Some(BinOpKind::Lt),
        IrBuiltin::LessThanEqualsByteString => Some(BinOpKind::Lte),
        IrBuiltin::EqualsString => Some(BinOpKind::Eq),
        IrBuiltin::EqualsData => Some(BinOpKind::Eq),
        IrBuiltin::AppendByteString => Some(BinOpKind::Append),
        IrBuiltin::AppendString => Some(BinOpKind::Append),
        _ => None,
    }
}

fn extract_builtin(node: &IrNode) -> Option<&IrBuiltin> {
    match node {
        IrNode::Builtin(b) => Some(b),
        IrNode::Force(inner) => extract_builtin(inner),
        _ => None,
    }
}

/// Generic helper: apply a transformation function to all children of a node
fn map_children(node: IrNode, f: fn(IrNode) -> IrNode) -> IrNode {
    match node {
        IrNode::Lambda { param_name, body } => IrNode::Lambda {
            param_name,
            body: Box::new(f(*body)),
        },
        IrNode::Apply {
            function,
            argument,
        } => IrNode::Apply {
            function: Box::new(f(*function)),
            argument: Box::new(f(*argument)),
        },
        IrNode::Force(inner) => IrNode::Force(Box::new(f(*inner))),
        IrNode::Delay(inner) => IrNode::Delay(Box::new(f(*inner))),
        IrNode::IfElse {
            condition,
            then_branch,
            else_branch,
        } => IrNode::IfElse {
            condition: Box::new(f(*condition)),
            then_branch: Box::new(f(*then_branch)),
            else_branch: Box::new(f(*else_branch)),
        },
        IrNode::LetBinding { name, value, body } => IrNode::LetBinding {
            name,
            value: Box::new(f(*value)),
            body: Box::new(f(*body)),
        },
        IrNode::BinOp { op, left, right } => IrNode::BinOp {
            op,
            left: Box::new(f(*left)),
            right: Box::new(f(*right)),
        },
        IrNode::UnaryOp { op, operand } => IrNode::UnaryOp {
            op,
            operand: Box::new(f(*operand)),
        },
        IrNode::Match { subject, branches } => IrNode::Match {
            subject: Box::new(f(*subject)),
            branches: branches
                .into_iter()
                .map(|b| MatchBranch {
                    pattern: b.pattern,
                    body: f(b.body),
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
            fields: fields.into_iter().map(f).collect(),
        },
        IrNode::Trace { message, body } => IrNode::Trace {
            message: Box::new(f(*message)),
            body: Box::new(f(*body)),
        },
        IrNode::Comment { text, node } => IrNode::Comment {
            text,
            node: Box::new(f(*node)),
        },
        IrNode::Expect {
            pattern,
            value,
            body,
        } => IrNode::Expect {
            pattern: Box::new(f(*pattern)),
            value: Box::new(f(*value)),
            body: Box::new(f(*body)),
        },
        IrNode::Block(items) => IrNode::Block(items.into_iter().map(f).collect()),
        IrNode::ListLit(items) => IrNode::ListLit(items.into_iter().map(f).collect()),
        IrNode::TupleLit(items) => IrNode::TupleLit(items.into_iter().map(f).collect()),
        // Leaf nodes
        other => other,
    }
}
