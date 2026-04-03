use crate::ir::*;

/// Recognize chains of constructor tag checks as pattern matching.
///
/// Aiken compiles `when x is { ... }` to chains of if-else:
/// ```
/// if 0 == constr_index(x) {
///   // branch for tag 0
/// } else {
///   if 1 == constr_index(x) {
///     // branch for tag 1
///   } else {
///     fail
///   }
/// }
/// ```
///
/// This pass collapses these chains into Match nodes:
/// ```
/// when x is {
///   Constr_0 -> ...
///   Constr_1 -> ...
///   _ -> fail
/// }
/// ```
pub fn recognize_pattern_matching(node: IrNode) -> IrNode {
    match node {
        IrNode::IfElse {
            condition,
            then_branch,
            else_branch,
        } => {
            // Try to extract a tag check from the condition
            if let Some((tag, subject)) = extract_tag_check(&condition) {
                // Check if the else branch is another tag check on the SAME subject,
                // or a fail/error
                let mut branches = vec![MatchBranch {
                    pattern: MatchPattern::Constructor {
                        tag,
                        type_hint: Some(tag_to_name(tag)),
                        bindings: Vec::new(),
                    },
                    body: recognize_pattern_matching(*then_branch),
                }];

                collect_match_branches(
                    *else_branch,
                    &subject,
                    &mut branches,
                );

                return IrNode::Match {
                    subject: Box::new(subject),
                    branches,
                };
            }

            // Not a tag check chain, recurse normally
            IrNode::IfElse {
                condition: Box::new(recognize_pattern_matching(*condition)),
                then_branch: Box::new(recognize_pattern_matching(*then_branch)),
                else_branch: Box::new(recognize_pattern_matching(*else_branch)),
            }
        }
        _ => map_children_pm(node, recognize_pattern_matching),
    }
}

/// Extract a tag check from a condition node.
/// Returns (tag_value, subject_being_checked) if the condition is:
/// - `N == constr_index(x)` or `constr_index(x) == N`
/// - `N == fst(un_constr_data(x))` or similar
/// - `N == var` where var is any variable (tag already extracted)
/// - A Comment wrapping such a check
fn extract_tag_check(node: &IrNode) -> Option<(usize, IrNode)> {
    match node {
        IrNode::BinOp {
            op: BinOpKind::Eq,
            left,
            right,
        } => {
            // N == constr_index(x)
            if let Some(n) = extract_int_value(left) {
                if let Some(subject) = extract_constr_index_subject(right) {
                    return Some((n as usize, subject));
                }
                // N == var (tag already in a variable)
                if is_variable_ref(right) {
                    return Some((n as usize, *right.clone()));
                }
            }
            // constr_index(x) == N or var == N
            if let Some(n) = extract_int_value(right) {
                if let Some(subject) = extract_constr_index_subject(left) {
                    return Some((n as usize, subject));
                }
                if is_variable_ref(left) {
                    return Some((n as usize, *left.clone()));
                }
            }
            None
        }
        IrNode::Comment { node, .. } => extract_tag_check(node),
        _ => None,
    }
}

fn is_variable_ref(node: &IrNode) -> bool {
    matches!(node, IrNode::Var(_))
}

/// Recursively collect match branches from nested if-else chains.
fn collect_match_branches(
    node: IrNode,
    expected_subject: &IrNode,
    branches: &mut Vec<MatchBranch>,
) {
    match node {
        IrNode::IfElse {
            condition,
            then_branch,
            else_branch,
        } => {
            if let Some((tag, subject)) = extract_tag_check(&condition) {
                // Check if we're matching on the same subject
                if subjects_match(&subject, expected_subject) {
                    branches.push(MatchBranch {
                        pattern: MatchPattern::Constructor {
                            tag,
                            type_hint: Some(tag_to_name(tag)),
                            bindings: Vec::new(),
                        },
                        body: recognize_pattern_matching(*then_branch),
                    });
                    collect_match_branches(*else_branch, expected_subject, branches);
                    return;
                }
            }
            // Not a matching tag check, add as wildcard
            branches.push(MatchBranch {
                pattern: MatchPattern::Wildcard,
                body: recognize_pattern_matching(IrNode::IfElse {
                    condition,
                    then_branch,
                    else_branch,
                }),
            });
        }
        IrNode::Error => {
            branches.push(MatchBranch {
                pattern: MatchPattern::Wildcard,
                body: IrNode::Error,
            });
        }
        IrNode::Apply { ref function, .. } if matches!(&**function, IrNode::Error) => {
            branches.push(MatchBranch {
                pattern: MatchPattern::Wildcard,
                body: IrNode::Error,
            });
        }
        other => {
            branches.push(MatchBranch {
                pattern: MatchPattern::Wildcard,
                body: recognize_pattern_matching(other),
            });
        }
    }
}

/// Check if two subjects refer to the same value.
/// This is a heuristic - we compare structural equality.
fn subjects_match(a: &IrNode, b: &IrNode) -> bool {
    // Simple structural comparison using Debug representation
    format!("{:?}", a) == format!("{:?}", b)
}

/// Extract an integer value from a node.
fn extract_int_value(node: &IrNode) -> Option<i128> {
    match node {
        IrNode::IntLit(n) => Some(*n),
        IrNode::Constant(IrConstant::Integer(n)) => Some(*n),
        _ => None,
    }
}

/// Extract the subject from a constr_index call.
/// Matches: constr_index(x), fst(un_constr_data(x)), or var references
/// that were bound to fstPair calls.
fn extract_constr_index_subject(node: &IrNode) -> Option<IrNode> {
    match node {
        IrNode::FnCall {
            function_name,
            args,
        } if function_name == "constr_index" && args.len() == 1 => {
            Some(args[0].clone())
        }
        // Also match Apply(fst_pair, Apply(un_constr_data, x))
        IrNode::Apply { function, argument } => {
            if is_fst_pair_like(function) {
                if let IrNode::Apply {
                    function: inner_fn,
                    argument: inner_arg,
                } = argument.as_ref()
                {
                    if is_un_constr_data_like(inner_fn) {
                        return Some(*inner_arg.clone());
                    }
                }
                if let IrNode::FnCall { function_name, args } = argument.as_ref() {
                    if function_name == "un_constr_data" && args.len() == 1 {
                        return Some(args[0].clone());
                    }
                }
            }
            None
        }
        _ => None,
    }
}

fn is_fst_pair_like(node: &IrNode) -> bool {
    match node {
        IrNode::Builtin(IrBuiltin::FstPair) => true,
        IrNode::Force(inner) => is_fst_pair_like(inner),
        _ => false,
    }
}

fn is_un_constr_data_like(node: &IrNode) -> bool {
    match node {
        IrNode::Builtin(IrBuiltin::UnConstrData) => true,
        IrNode::Force(inner) => is_un_constr_data_like(inner),
        _ => false,
    }
}

fn tag_to_name(tag: usize) -> String {
    format!("Constr_{}", tag)
}

fn map_children_pm(node: IrNode, f: fn(IrNode) -> IrNode) -> IrNode {
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
        IrNode::FnCall {
            function_name,
            args,
        } => IrNode::FnCall {
            function_name,
            args: args.into_iter().map(f).collect(),
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
        other => other,
    }
}
