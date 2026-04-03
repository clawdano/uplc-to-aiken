use crate::ir::*;

/// Recognize and unpack the "builtin pack" pattern used by Aiken V3.
///
/// In V3, Aiken packs frequently-used builtins into a Constr and unpacks them
/// once at the top level:
///
/// ```text
/// Case(Constr(0, [Force(TailList), Force(HeadList), Force(Force(SndPair)),
///                  Force(Force(FstPair)), Force(IfThenElse)]),
///      [Lambda^5 -> body])
/// ```
///
/// This is equivalent to:
/// ```text
/// let tail_list = Force(TailList)
/// let head_list = Force(HeadList)
/// ...
/// body
/// ```
///
/// We replace this with the body, substituting the De Bruijn references to
/// the builtins with their actual values. For now, we convert it to nested
/// let-bindings.
pub fn unpack_builtin_pack(node: IrNode) -> IrNode {
    match node {
        IrNode::Match {
            subject,
            ref branches,
        } if branches.len() == 1 => {
            if let IrNode::Constr {
                tag: 0,
                ref fields,
                ..
            } = *subject
            {
                // Check if all fields are builtins (possibly forced)
                if fields.iter().all(|f| is_builtin_like(f)) {
                    // Extract the body from the single branch
                    let body = &branches[0].body;

                    // Count how many lambdas wrap the body (should match field count)
                    let (lambda_count, inner_body) = unwrap_lambdas(body);

                    if lambda_count == fields.len() {
                        // Create nested let-bindings for each builtin
                        let mut result = unpack_builtin_pack(inner_body.clone());

                        // Bind in reverse order so De Bruijn indices work out
                        for (i, field) in fields.iter().enumerate().rev() {
                            let name = builtin_field_name(field, i);
                            result = IrNode::LetBinding {
                                name,
                                value: Box::new(field.clone()),
                                body: Box::new(result),
                            };
                        }

                        return result;
                    }
                }
            }

            // Not a builtin pack, recurse
            IrNode::Match {
                subject: Box::new(unpack_builtin_pack(*subject)),
                branches: branches
                    .iter()
                    .cloned()
                    .map(|b| MatchBranch {
                        pattern: b.pattern,
                        body: unpack_builtin_pack(b.body),
                    })
                    .collect(),
            }
        }

        _ => map_children_v3(node, unpack_builtin_pack),
    }
}

/// Recognize V3 if-then-else pattern:
///
/// ```text
/// Force(Case(Constr(0, [condition, Delay(then), Delay(else)]), [selector_var]))
/// ```
///
/// In V3, Aiken compiles if-then-else by packing condition + delayed branches
/// into a Constr, then using Case to select. The selector is typically the
/// IfThenElse builtin variable.
pub fn recognize_v3_if_then_else(node: IrNode) -> IrNode {
    match node {
        IrNode::Force(inner) => {
            if let IrNode::Match {
                subject,
                ref branches,
            } = *inner
            {
                if branches.len() == 1 {
                    if let IrNode::Constr {
                        tag: 0,
                        ref fields,
                        ..
                    } = *subject
                    {
                        if fields.len() == 3 {
                            let condition = &fields[0];
                            let then_branch = &fields[1];
                            let else_branch = &fields[2];

                            // Check if then/else are delayed
                            let then_body = match then_branch {
                                IrNode::Delay(inner) => (**inner).clone(),
                                other => other.clone(),
                            };
                            let else_body = match else_branch {
                                IrNode::Delay(inner) => (**inner).clone(),
                                other => other.clone(),
                            };

                            return IrNode::IfElse {
                                condition: Box::new(recognize_v3_if_then_else(condition.clone())),
                                then_branch: Box::new(recognize_v3_if_then_else(then_body)),
                                else_branch: Box::new(recognize_v3_if_then_else(else_body)),
                            };
                        }
                    }
                }

                return IrNode::Force(Box::new(IrNode::Match {
                    subject: Box::new(recognize_v3_if_then_else(*subject)),
                    branches: branches
                        .iter()
                        .cloned()
                        .map(|b| MatchBranch {
                            pattern: b.pattern,
                            body: recognize_v3_if_then_else(b.body),
                        })
                        .collect(),
                }));
            }

            IrNode::Force(Box::new(recognize_v3_if_then_else(*inner)))
        }

        _ => map_children_v3(node, recognize_v3_if_then_else),
    }
}

/// Recognize general Constr/Case destructuring pattern:
///
/// ```text
/// Case(Constr(0, [field1, field2, ...]), [Lambda^n -> body])
/// ```
///
/// This is equivalent to: let (f1, f2, ...) = (field1, field2, ...) in body
///
/// We convert these to nested let-bindings.
pub fn recognize_constr_case_destruct(node: IrNode) -> IrNode {
    match node {
        IrNode::Match {
            subject,
            ref branches,
        } if branches.len() == 1 => {
            if let IrNode::Constr {
                tag: 0,
                ref fields,
                ..
            } = *subject
            {
                let body = &branches[0].body;
                let (lambda_count, inner_body) = unwrap_lambdas(body);

                if lambda_count == fields.len() && lambda_count > 0 {
                    let mut result = recognize_constr_case_destruct(inner_body.clone());

                    for (i, field) in fields.iter().enumerate().rev() {
                        result = IrNode::LetBinding {
                            name: format!("field_{}", i),
                            value: Box::new(recognize_constr_case_destruct(field.clone())),
                            body: Box::new(result),
                        };
                    }

                    return result;
                }
            }

            IrNode::Match {
                subject: Box::new(recognize_constr_case_destruct(*subject)),
                branches: branches
                    .iter()
                    .cloned()
                    .map(|b| MatchBranch {
                        pattern: b.pattern,
                        body: recognize_constr_case_destruct(b.body),
                    })
                    .collect(),
            }
        }

        _ => map_children_v3(node, recognize_constr_case_destruct),
    }
}

// === Helpers ===

fn is_builtin_like(node: &IrNode) -> bool {
    match node {
        IrNode::Builtin(_) => true,
        IrNode::Force(inner) => is_builtin_like(inner),
        _ => false,
    }
}

fn unwrap_lambdas(node: &IrNode) -> (usize, &IrNode) {
    match node {
        IrNode::Lambda { body, .. } => {
            let (count, inner) = unwrap_lambdas(body);
            (count + 1, inner)
        }
        other => (0, other),
    }
}

fn builtin_field_name(node: &IrNode, index: usize) -> String {
    match extract_builtin_name(node) {
        Some(name) => name,
        None => format!("builtin_{}", index),
    }
}

fn extract_builtin_name(node: &IrNode) -> Option<String> {
    match node {
        IrNode::Builtin(b) => Some(short_builtin_name(b)),
        IrNode::Force(inner) => extract_builtin_name(inner),
        _ => None,
    }
}

fn short_builtin_name(b: &IrBuiltin) -> String {
    match b {
        IrBuiltin::TailList => "tail_list".to_string(),
        IrBuiltin::HeadList => "head_list".to_string(),
        IrBuiltin::SndPair => "snd_pair".to_string(),
        IrBuiltin::FstPair => "fst_pair".to_string(),
        IrBuiltin::IfThenElse => "if_then_else".to_string(),
        IrBuiltin::NullList => "null_list".to_string(),
        IrBuiltin::ChooseList => "choose_list".to_string(),
        IrBuiltin::ChooseData => "choose_data".to_string(),
        IrBuiltin::ChooseUnit => "choose_unit".to_string(),
        IrBuiltin::MkCons => "mk_cons".to_string(),
        IrBuiltin::Trace => "trace_fn".to_string(),
        IrBuiltin::UnConstrData => "un_constr_data".to_string(),
        IrBuiltin::UnMapData => "un_map_data".to_string(),
        IrBuiltin::UnListData => "un_list_data".to_string(),
        IrBuiltin::UnIData => "un_i_data".to_string(),
        IrBuiltin::UnBData => "un_b_data".to_string(),
        IrBuiltin::ConstrData => "constr_data".to_string(),
        IrBuiltin::MapData => "map_data".to_string(),
        IrBuiltin::ListData => "list_data".to_string(),
        IrBuiltin::IData => "i_data".to_string(),
        IrBuiltin::BData => "b_data".to_string(),
        IrBuiltin::EqualsData => "equals_data".to_string(),
        other => format!("{:?}", other).to_lowercase(),
    }
}

fn map_children_v3(node: IrNode, f: fn(IrNode) -> IrNode) -> IrNode {
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
        other => other,
    }
}
