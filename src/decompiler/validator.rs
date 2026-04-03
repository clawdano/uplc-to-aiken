use crate::ir::*;

/// Recognize the validator wrapper pattern and clean it up.
///
/// After V3 passes, the structure is:
/// ```
/// fn(multi_arg) {
///   let tail_list = ...
///   let head_list = ...
///   ...
///   <validator body with args>
/// }
/// ```
///
/// This pass strips the outermost lambda and implementation-detail let-bindings,
/// adjusting De Bruijn indices to account for removed binders.
pub fn recognize_validator(node: IrNode) -> IrNode {
    match node {
        IrNode::Lambda { body, .. } => {
            // Strip the outermost lambda (multi-validator dispatch)
            // Shift De Bruijn indices down by 1 to account for removed lambda
            let body = shift_debruijn(*body, -1, 1);
            let body = strip_builtin_lets(body);
            // Strip the multi-validator dispatch wrapper:
            // `if <body> { Void } else { fail(fail) }` -> <body>
            strip_dispatch_wrapper(body)
        }
        other => other,
    }
}

/// Strip the multi-validator dispatch wrapper.
///
/// Aiken wraps validators in:
/// ```
/// if <validator_body> { Void } else { fail(fail) }
/// ```
/// This is the dispatch mechanism - we strip it to expose the body.
fn strip_dispatch_wrapper(node: IrNode) -> IrNode {
    match node {
        IrNode::IfElse {
            condition,
            then_branch,
            else_branch,
        } => {
            // Check if else is `fail(fail)` or similar error pattern
            let is_fail_else = is_fail_pattern(&else_branch);
            // Check if then is Void/Unit
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

/// Strip the builtin let-bindings that are Aiken implementation details.
/// Each time we strip a let-binding, we shift De Bruijn indices down
/// to account for the removed binder.
fn strip_builtin_lets(node: IrNode) -> IrNode {
    match node {
        IrNode::LetBinding {
            ref name,
            value: _,
            ref body,
        } if is_builtin_let_name(name) => {
            // Remove this binding and shift De Bruijn indices in the body
            let body = shift_debruijn(*body.clone(), -1, 1);
            strip_builtin_lets(body)
        }
        other => other,
    }
}

/// Shift De Bruijn indices in an IR node.
///
/// `delta`: how much to shift (negative = decrement, positive = increment)
/// `cutoff`: only shift variables with index >= cutoff (to avoid shifting bound vars)
fn shift_debruijn(node: IrNode, delta: i64, cutoff: usize) -> IrNode {
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

        // Leaf nodes
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
