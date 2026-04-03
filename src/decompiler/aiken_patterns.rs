use crate::ir::*;

/// Recognize Aiken-specific compilation patterns and replace them with
/// higher-level constructs.
///
/// Key patterns recognized:
/// 1. CONSTR_FIELDS_EXPOSER: `sndPair(unConstrData(x))` -> field list access
/// 2. CONSTR_INDEX_EXPOSER: `fstPair(unConstrData(x))` -> constructor tag
/// 3. Field access: `headList(tailList^n(fields))` -> `x.field_n`
/// 4. Constructor tag check: `equalsInteger(N, fstPair(unConstrData(x)))` -> tag match
/// 5. Option Some/None: tag 0 = None, tag 1 = Some (with datum check pattern)
/// 6. Bool: tag 0 = False, tag 1 = True

/// Run all Aiken pattern recognition passes
pub fn recognize_aiken_patterns(node: IrNode) -> IrNode {
    let node = recognize_constr_accessors(node);
    let node = recognize_field_access(node);
    let node = recognize_tag_checks(node);
    let node = recognize_datum_check(node);
    node
}

/// Recognize `sndPair(unConstrData(x))` as accessing constructor fields
/// and `fstPair(unConstrData(x))` as accessing constructor tag.
///
/// These are the two most common patterns in Aiken-compiled UPLC.
fn recognize_constr_accessors(node: IrNode) -> IrNode {
    match node {
        // Apply(Apply(Force(Force(SndPair)), Apply(UnConstrData, x)))
        // -> ConstrFields(x)
        IrNode::Apply {
            function,
            argument,
        } => {
            let argument = recognize_constr_accessors(*argument);

            // Check for sndPair(unConstrData(x)) pattern
            if is_snd_pair(&function) {
                if let IrNode::Apply {
                    function: inner_fn,
                    argument: inner_arg,
                } = &argument
                {
                    if is_un_constr_data(inner_fn) {
                        return IrNode::FnCall {
                            function_name: "constr_fields".to_string(),
                            args: vec![*inner_arg.clone()],
                        };
                    }
                }
                // sndPair applied to something else
                return IrNode::FnCall {
                    function_name: "snd".to_string(),
                    args: vec![argument],
                };
            }

            // Check for fstPair(unConstrData(x)) pattern
            if is_fst_pair(&function) {
                if let IrNode::Apply {
                    function: inner_fn,
                    argument: inner_arg,
                } = &argument
                {
                    if is_un_constr_data(inner_fn) {
                        return IrNode::FnCall {
                            function_name: "constr_index".to_string(),
                            args: vec![*inner_arg.clone()],
                        };
                    }
                }
                // fstPair applied to something else
                return IrNode::FnCall {
                    function_name: "fst".to_string(),
                    args: vec![argument],
                };
            }

            // Check for plain unConstrData
            if is_un_constr_data(&function) {
                return IrNode::FnCall {
                    function_name: "un_constr_data".to_string(),
                    args: vec![argument],
                };
            }

            IrNode::Apply {
                function: Box::new(recognize_constr_accessors(*function)),
                argument: Box::new(argument),
            }
        }
        _ => map_children_ak(node, recognize_constr_accessors),
    }
}

/// Recognize field access patterns:
/// - `headList(fields)` -> `fields[0]` or `x.field_0`
/// - `headList(tailList(fields))` -> `fields[1]` or `x.field_1`
/// - `headList(tailList(tailList(fields)))` -> `fields[2]` or `x.field_2`
fn recognize_field_access(node: IrNode) -> IrNode {
    match node {
        IrNode::UnaryOp {
            op: UnaryOpKind::Head,
            operand,
        } => {
            let operand = recognize_field_access(*operand);
            let (tail_count, inner) = count_tails(&operand);

            // Check if the inner expression is a constr_fields call
            if let IrNode::FnCall {
                ref function_name,
                ref args,
            } = inner
            {
                if function_name == "constr_fields" && args.len() == 1 {
                    return IrNode::FieldAccess {
                        record: Box::new(args[0].clone()),
                        field_index: tail_count,
                        field_name: None,
                    };
                }
            }

            // Check if it's a let-bound fields variable
            if tail_count > 0 {
                IrNode::FieldAccess {
                    record: Box::new(inner.clone()),
                    field_index: tail_count,
                    field_name: None,
                }
            } else {
                IrNode::UnaryOp {
                    op: UnaryOpKind::Head,
                    operand: Box::new(operand),
                }
            }
        }
        _ => map_children_ak(node, recognize_field_access),
    }
}

/// Recognize constructor tag check patterns:
/// `equalsInteger(N, constr_index(x))` or `N == constr_index(x)`
/// These represent pattern matching on constructor tags.
fn recognize_tag_checks(node: IrNode) -> IrNode {
    match node {
        IrNode::BinOp {
            op: BinOpKind::Eq,
            left,
            right,
        } => {
            let left = recognize_tag_checks(*left);
            let right = recognize_tag_checks(*right);

            // Check for N == constr_index(x) or constr_index(x) == N
            if let (IrNode::IntLit(n), IrNode::FnCall { ref function_name, .. })
            | (IrNode::Constant(IrConstant::Integer(n)), IrNode::FnCall { ref function_name, .. }) =
                (&left, &right)
            {
                if function_name == "constr_index" {
                    return IrNode::Comment {
                        text: format!("tag == {} ({})", n, tag_hint(*n)),
                        node: Box::new(IrNode::BinOp {
                            op: BinOpKind::Eq,
                            left: Box::new(left),
                            right: Box::new(right),
                        }),
                    };
                }
            }

            if let (IrNode::FnCall { ref function_name, .. }, IrNode::IntLit(n))
            | (IrNode::FnCall { ref function_name, .. }, IrNode::Constant(IrConstant::Integer(n))) =
                (&left, &right)
            {
                if function_name == "constr_index" {
                    return IrNode::Comment {
                        text: format!("tag == {} ({})", n, tag_hint(*n)),
                        node: Box::new(IrNode::BinOp {
                            op: BinOpKind::Eq,
                            left: Box::new(left),
                            right: Box::new(right),
                        }),
                    };
                }
            }

            IrNode::BinOp {
                op: BinOpKind::Eq,
                left: Box::new(left),
                right: Box::new(right),
            }
        }
        _ => map_children_ak(node, recognize_tag_checks),
    }
}

/// Recognize the datum Some/None check pattern.
///
/// Aiken validators with `Option<Data>` datum compile a check:
/// `1 == constr_index(datum)` which means "datum is Some"
///
/// The outer if-else is:
/// ```
/// if 1 == constr_index(datum) {
///   // datum is Some, extract the value
///   ...
/// } else {
///   fail  // datum is None, this spend handler doesn't apply
/// }
/// ```
fn recognize_datum_check(node: IrNode) -> IrNode {
    match node {
        IrNode::IfElse {
            condition,
            then_branch,
            else_branch,
        } => {
            let condition = recognize_datum_check(*condition);
            let then_branch = recognize_datum_check(*then_branch);
            let else_branch = recognize_datum_check(*else_branch);

            // Check if this is a datum Some check
            if is_datum_some_check(&condition) && is_fail_like(&else_branch) {
                return IrNode::Comment {
                    text: "expect Some(datum_value) = datum".to_string(),
                    node: Box::new(then_branch),
                };
            }

            IrNode::IfElse {
                condition: Box::new(condition),
                then_branch: Box::new(then_branch),
                else_branch: Box::new(else_branch),
            }
        }
        _ => map_children_ak(node, recognize_datum_check),
    }
}

// === Helpers ===

fn is_snd_pair(node: &IrNode) -> bool {
    match node {
        IrNode::Builtin(IrBuiltin::SndPair) => true,
        IrNode::Force(inner) => is_snd_pair(inner),
        // Also match var references that were bound to snd_pair
        IrNode::Var(_) => false, // Can't resolve these statically
        _ => false,
    }
}

fn is_fst_pair(node: &IrNode) -> bool {
    match node {
        IrNode::Builtin(IrBuiltin::FstPair) => true,
        IrNode::Force(inner) => is_fst_pair(inner),
        _ => false,
    }
}

fn is_un_constr_data(node: &IrNode) -> bool {
    match node {
        IrNode::Builtin(IrBuiltin::UnConstrData) => true,
        IrNode::Force(inner) => is_un_constr_data(inner),
        _ => false,
    }
}

/// Count the number of tail operations wrapping an expression.
/// Returns (count, innermost_expression).
fn count_tails(node: &IrNode) -> (usize, &IrNode) {
    match node {
        IrNode::UnaryOp {
            op: UnaryOpKind::Tail,
            operand,
        } => {
            let (count, inner) = count_tails(operand);
            (count + 1, inner)
        }
        other => (0, other),
    }
}

fn tag_hint(tag: i128) -> &'static str {
    match tag {
        0 => "possibly False/None/first constructor",
        1 => "possibly True/Some/second constructor",
        2 => "third constructor",
        3 => "fourth constructor",
        _ => "constructor",
    }
}

fn is_datum_some_check(node: &IrNode) -> bool {
    // Pattern: 1 == constr_index(x) or Comment wrapping this
    match node {
        IrNode::BinOp {
            op: BinOpKind::Eq,
            left,
            right,
        } => {
            let has_one = matches!(**left, IrNode::IntLit(1) | IrNode::Constant(IrConstant::Integer(1)))
                || matches!(**right, IrNode::IntLit(1) | IrNode::Constant(IrConstant::Integer(1)));
            let has_constr_index = is_constr_index_call(left) || is_constr_index_call(right);
            has_one && has_constr_index
        }
        IrNode::Comment { node, .. } => is_datum_some_check(node),
        _ => false,
    }
}

fn is_constr_index_call(node: &IrNode) -> bool {
    match node {
        IrNode::FnCall {
            function_name,
            ..
        } => function_name == "constr_index",
        IrNode::Apply { function, argument } => {
            // Also match the raw pattern: fstPair(unConstrData(x))
            is_fst_pair(function)
                && matches!(
                    **argument,
                    IrNode::Apply { .. } | IrNode::FnCall { .. }
                )
        }
        _ => false,
    }
}

fn is_fail_like(node: &IrNode) -> bool {
    match node {
        IrNode::Error => true,
        IrNode::Apply { function, .. } => matches!(**function, IrNode::Error),
        _ => false,
    }
}

fn map_children_ak(node: IrNode, f: fn(IrNode) -> IrNode) -> IrNode {
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
        IrNode::FnCall {
            function_name,
            args,
        } => IrNode::FnCall {
            function_name,
            args: args.into_iter().map(f).collect(),
        },
        IrNode::FnDef {
            name,
            params,
            body,
        } => IrNode::FnDef {
            name,
            params,
            body: Box::new(f(*body)),
        },
        IrNode::Validator {
            name,
            params,
            body,
        } => IrNode::Validator {
            name,
            params,
            body: Box::new(f(*body)),
        },
        IrNode::FieldAccess {
            record,
            field_index,
            field_name,
        } => IrNode::FieldAccess {
            record: Box::new(f(*record)),
            field_index,
            field_name,
        },
        other => other,
    }
}
