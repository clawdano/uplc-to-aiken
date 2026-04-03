use crate::ir::*;

/// Recognize the validator wrapper pattern and clean it up.
///
/// This pass:
/// 1. Strips the outermost lambda (multi-validator dispatch)
/// 2. Inlines builtin let-bindings (replaces var refs with actual builtins)
/// 3. Strips the dispatch wrapper (`if body { Void } else { fail }`)
pub fn recognize_validator(node: IrNode) -> IrNode {
    match node {
        IrNode::Lambda { body, .. } => {
            // Inline builtin let-bindings FIRST (before removing outer lambda)
            let body = inline_builtin_lets(*body);
            // Now strip the outermost lambda
            let body = shift_debruijn(body, -1, 1);
            // Strip the multi-validator dispatch wrapper
            strip_dispatch_wrapper(body)
        }
        other => other,
    }
}

/// Inline builtin let-bindings: replace the let-binding with its body,
/// substituting all var references to the binding with the actual value.
fn inline_builtin_lets(node: IrNode) -> IrNode {
    match node {
        IrNode::LetBinding {
            ref name,
            ref value,
            ref body,
        } if is_builtin_let_name(name) => {
            // Substitute var_1 (the innermost binding) with the value in the body,
            // then shift all other vars down by 1
            let inlined = substitute(*body.clone(), 1, *value.clone());
            let shifted = shift_debruijn(inlined, -1, 1);
            inline_builtin_lets(shifted)
        }
        other => other,
    }
}

/// Substitute all occurrences of Var(target_index) with replacement in node.
/// Adjusts indices when entering binders.
fn substitute(node: IrNode, target_index: usize, replacement: IrNode) -> IrNode {
    match node {
        IrNode::Var(index) => {
            if index == target_index {
                replacement.clone()
            } else {
                IrNode::Var(index)
            }
        }

        IrNode::Lambda { param_name, body } => IrNode::Lambda {
            param_name,
            body: Box::new(substitute(
                *body,
                target_index + 1,
                shift_debruijn(replacement, 1, 1),
            )),
        },

        IrNode::LetBinding { name, value, body } => IrNode::LetBinding {
            name,
            value: Box::new(substitute(*value, target_index, replacement.clone())),
            body: Box::new(substitute(
                *body,
                target_index + 1,
                shift_debruijn(replacement, 1, 1),
            )),
        },

        IrNode::Apply {
            function,
            argument,
        } => IrNode::Apply {
            function: Box::new(substitute(*function, target_index, replacement.clone())),
            argument: Box::new(substitute(*argument, target_index, replacement)),
        },

        IrNode::IfElse {
            condition,
            then_branch,
            else_branch,
        } => IrNode::IfElse {
            condition: Box::new(substitute(*condition, target_index, replacement.clone())),
            then_branch: Box::new(substitute(*then_branch, target_index, replacement.clone())),
            else_branch: Box::new(substitute(*else_branch, target_index, replacement)),
        },

        IrNode::Force(inner) => {
            IrNode::Force(Box::new(substitute(*inner, target_index, replacement)))
        }
        IrNode::Delay(inner) => {
            IrNode::Delay(Box::new(substitute(*inner, target_index, replacement)))
        }

        IrNode::BinOp { op, left, right } => IrNode::BinOp {
            op,
            left: Box::new(substitute(*left, target_index, replacement.clone())),
            right: Box::new(substitute(*right, target_index, replacement)),
        },

        IrNode::UnaryOp { op, operand } => IrNode::UnaryOp {
            op,
            operand: Box::new(substitute(*operand, target_index, replacement)),
        },

        IrNode::Trace { message, body } => IrNode::Trace {
            message: Box::new(substitute(*message, target_index, replacement.clone())),
            body: Box::new(substitute(*body, target_index, replacement)),
        },

        IrNode::Match { subject, branches } => IrNode::Match {
            subject: Box::new(substitute(*subject, target_index, replacement.clone())),
            branches: branches
                .into_iter()
                .map(|b| MatchBranch {
                    pattern: b.pattern,
                    body: substitute(b.body, target_index, replacement.clone()),
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
            fields: fields
                .into_iter()
                .map(|f| substitute(f, target_index, replacement.clone()))
                .collect(),
        },

        IrNode::Comment { text, node } => IrNode::Comment {
            text,
            node: Box::new(substitute(*node, target_index, replacement)),
        },

        IrNode::Block(items) => IrNode::Block(
            items
                .into_iter()
                .map(|i| substitute(i, target_index, replacement.clone()))
                .collect(),
        ),

        // Leaf nodes
        other => other,
    }
}

/// Strip the multi-validator dispatch wrapper.
fn strip_dispatch_wrapper(node: IrNode) -> IrNode {
    match node {
        IrNode::IfElse {
            condition,
            then_branch,
            else_branch,
        } => {
            let is_fail_else = is_fail_pattern(&else_branch);
            let is_void_then = matches!(
                *then_branch,
                IrNode::Unit | IrNode::Constant(IrConstant::Unit)
            );

            if is_void_then && is_fail_else {
                return *condition;
            }

            IrNode::IfElse {
                condition,
                then_branch,
                else_branch,
            }
        }
        other => other,
    }
}

fn is_fail_pattern(node: &IrNode) -> bool {
    match node {
        IrNode::Error => true,
        IrNode::Apply { function, .. } => matches!(**function, IrNode::Error),
        _ => false,
    }
}

/// Shift De Bruijn indices in an IR node.
pub fn shift_debruijn(node: IrNode, delta: i64, cutoff: usize) -> IrNode {
    match node {
        IrNode::Var(index) => {
            if index >= cutoff {
                let new_index = (index as i64 + delta).max(0) as usize;
                IrNode::Var(new_index)
            } else {
                IrNode::Var(index)
            }
        }

        IrNode::Lambda { param_name, body } => IrNode::Lambda {
            param_name,
            body: Box::new(shift_debruijn(*body, delta, cutoff + 1)),
        },

        IrNode::LetBinding { name, value, body } => IrNode::LetBinding {
            name,
            value: Box::new(shift_debruijn(*value, delta, cutoff)),
            body: Box::new(shift_debruijn(*body, delta, cutoff + 1)),
        },

        IrNode::Apply {
            function,
            argument,
        } => IrNode::Apply {
            function: Box::new(shift_debruijn(*function, delta, cutoff)),
            argument: Box::new(shift_debruijn(*argument, delta, cutoff)),
        },

        IrNode::IfElse {
            condition,
            then_branch,
            else_branch,
        } => IrNode::IfElse {
            condition: Box::new(shift_debruijn(*condition, delta, cutoff)),
            then_branch: Box::new(shift_debruijn(*then_branch, delta, cutoff)),
            else_branch: Box::new(shift_debruijn(*else_branch, delta, cutoff)),
        },

        IrNode::Force(inner) => IrNode::Force(Box::new(shift_debruijn(*inner, delta, cutoff))),
        IrNode::Delay(inner) => IrNode::Delay(Box::new(shift_debruijn(*inner, delta, cutoff))),

        IrNode::BinOp { op, left, right } => IrNode::BinOp {
            op,
            left: Box::new(shift_debruijn(*left, delta, cutoff)),
            right: Box::new(shift_debruijn(*right, delta, cutoff)),
        },

        IrNode::UnaryOp { op, operand } => IrNode::UnaryOp {
            op,
            operand: Box::new(shift_debruijn(*operand, delta, cutoff)),
        },

        IrNode::Trace { message, body } => IrNode::Trace {
            message: Box::new(shift_debruijn(*message, delta, cutoff)),
            body: Box::new(shift_debruijn(*body, delta, cutoff)),
        },

        IrNode::Match { subject, branches } => IrNode::Match {
            subject: Box::new(shift_debruijn(*subject, delta, cutoff)),
            branches: branches
                .into_iter()
                .map(|b| MatchBranch {
                    pattern: b.pattern,
                    body: shift_debruijn(b.body, delta, cutoff),
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
            fields: fields
                .into_iter()
                .map(|f| shift_debruijn(f, delta, cutoff))
                .collect(),
        },

        IrNode::Comment { text, node } => IrNode::Comment {
            text,
            node: Box::new(shift_debruijn(*node, delta, cutoff)),
        },

        IrNode::Expect {
            pattern,
            value,
            body,
        } => IrNode::Expect {
            pattern: Box::new(shift_debruijn(*pattern, delta, cutoff)),
            value: Box::new(shift_debruijn(*value, delta, cutoff)),
            body: Box::new(shift_debruijn(*body, delta, cutoff + 1)),
        },

        IrNode::Block(items) => IrNode::Block(
            items
                .into_iter()
                .map(|i| shift_debruijn(i, delta, cutoff))
                .collect(),
        ),

        IrNode::ListLit(items) => IrNode::ListLit(
            items
                .into_iter()
                .map(|i| shift_debruijn(i, delta, cutoff))
                .collect(),
        ),

        IrNode::TupleLit(items) => IrNode::TupleLit(
            items
                .into_iter()
                .map(|i| shift_debruijn(i, delta, cutoff))
                .collect(),
        ),

        other => other,
    }
}

fn is_builtin_let_name(name: &str) -> bool {
    matches!(
        name,
        "tail_list"
            | "head_list"
            | "snd_pair"
            | "fst_pair"
            | "if_then_else"
            | "null_list"
            | "choose_list"
            | "choose_data"
            | "choose_unit"
            | "mk_cons"
            | "trace_fn"
    )
}
