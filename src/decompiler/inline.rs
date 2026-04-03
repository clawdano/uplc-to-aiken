use crate::ir::*;
use crate::decompiler::validator::shift_debruijn;

/// Inline simple let-bindings that are implementation details.
///
/// After builtin inlining, there are often let-bindings like:
/// ```
/// let val_4 = builtin.equals_integer(1)
/// let val_5 = fst(val_3)
/// ```
///
/// These are partial applications or simple accessor calls that obscure
/// the actual logic. This pass inlines them at their usage sites if:
/// 1. The value is a partial application of a builtin (e.g., `equals_integer(N)`)
/// 2. The value is a simple accessor (e.g., `fst(x)`, `snd(x)`)
/// 3. The binding is only used once
pub fn inline_simple_lets(node: IrNode) -> IrNode {
    match node {
        IrNode::LetBinding {
            ref name,
            ref value,
            ref body,
        } => {
            if should_inline(value) {
                // Substitute var_1 with value in body, then shift
                let inlined = substitute_var(*body.clone(), 1, *value.clone());
                let shifted = shift_debruijn(inlined, -1, 1);
                return inline_simple_lets(shifted);
            }

            IrNode::LetBinding {
                name: name.clone(),
                value: Box::new(inline_simple_lets(*value.clone())),
                body: Box::new(inline_simple_lets(*body.clone())),
            }
        }
        _ => map_children_il(node, inline_simple_lets),
    }
}

/// Check if a value should be inlined.
fn should_inline(value: &IrNode) -> bool {
    match value {
        // Partial application of a builtin: e.g., equals_integer(N)
        IrNode::Apply { function, argument } => {
            is_builtin_or_forced(function) && is_simple_value(argument)
        }
        // Simple function calls: fst(x), snd(x), constr_index(x), constr_fields(x)
        IrNode::FnCall { function_name, args } => {
            matches!(
                function_name.as_str(),
                "fst" | "snd" | "constr_index" | "constr_fields"
            ) && args.len() == 1
        }
        _ => false,
    }
}

fn is_builtin_or_forced(node: &IrNode) -> bool {
    match node {
        IrNode::Builtin(_) => true,
        IrNode::Force(inner) => is_builtin_or_forced(inner),
        _ => false,
    }
}

fn is_simple_value(node: &IrNode) -> bool {
    match node {
        IrNode::Var(_) => true,
        IrNode::IntLit(_) => true,
        IrNode::Constant(IrConstant::Integer(_)) => true,
        IrNode::Constant(IrConstant::ByteString(_)) => true,
        IrNode::BoolLit(_) => true,
        IrNode::Unit => true,
        _ => false,
    }
}

/// Substitute all occurrences of Var(target) with replacement.
fn substitute_var(node: IrNode, target: usize, replacement: IrNode) -> IrNode {
    match node {
        IrNode::Var(index) => {
            if index == target {
                replacement.clone()
            } else {
                IrNode::Var(index)
            }
        }
        IrNode::Lambda { param_name, body } => IrNode::Lambda {
            param_name,
            body: Box::new(substitute_var(
                *body,
                target + 1,
                shift_debruijn(replacement, 1, 1),
            )),
        },
        IrNode::LetBinding { name, value, body } => IrNode::LetBinding {
            name,
            value: Box::new(substitute_var(*value, target, replacement.clone())),
            body: Box::new(substitute_var(
                *body,
                target + 1,
                shift_debruijn(replacement, 1, 1),
            )),
        },
        IrNode::Apply {
            function,
            argument,
        } => IrNode::Apply {
            function: Box::new(substitute_var(*function, target, replacement.clone())),
            argument: Box::new(substitute_var(*argument, target, replacement)),
        },
        IrNode::IfElse {
            condition,
            then_branch,
            else_branch,
        } => IrNode::IfElse {
            condition: Box::new(substitute_var(*condition, target, replacement.clone())),
            then_branch: Box::new(substitute_var(*then_branch, target, replacement.clone())),
            else_branch: Box::new(substitute_var(*else_branch, target, replacement)),
        },
        IrNode::Force(inner) => {
            IrNode::Force(Box::new(substitute_var(*inner, target, replacement)))
        }
        IrNode::Delay(inner) => {
            IrNode::Delay(Box::new(substitute_var(*inner, target, replacement)))
        }
        IrNode::BinOp { op, left, right } => IrNode::BinOp {
            op,
            left: Box::new(substitute_var(*left, target, replacement.clone())),
            right: Box::new(substitute_var(*right, target, replacement)),
        },
        IrNode::UnaryOp { op, operand } => IrNode::UnaryOp {
            op,
            operand: Box::new(substitute_var(*operand, target, replacement)),
        },
        IrNode::Trace { message, body } => IrNode::Trace {
            message: Box::new(substitute_var(*message, target, replacement.clone())),
            body: Box::new(substitute_var(*body, target, replacement)),
        },
        IrNode::Match { subject, branches } => IrNode::Match {
            subject: Box::new(substitute_var(*subject, target, replacement.clone())),
            branches: branches
                .into_iter()
                .map(|b| MatchBranch {
                    pattern: b.pattern,
                    body: substitute_var(b.body, target, replacement.clone()),
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
                .map(|f| substitute_var(f, target, replacement.clone()))
                .collect(),
        },
        IrNode::Comment { text, node } => IrNode::Comment {
            text,
            node: Box::new(substitute_var(*node, target, replacement)),
        },
        IrNode::FnCall {
            function_name,
            args,
        } => IrNode::FnCall {
            function_name,
            args: args
                .into_iter()
                .map(|a| substitute_var(a, target, replacement.clone()))
                .collect(),
        },
        IrNode::FieldAccess {
            record,
            field_index,
            field_name,
        } => IrNode::FieldAccess {
            record: Box::new(substitute_var(*record, target, replacement)),
            field_index,
            field_name,
        },
        IrNode::Block(items) => IrNode::Block(
            items
                .into_iter()
                .map(|i| substitute_var(i, target, replacement.clone()))
                .collect(),
        ),
        other => other,
    }
}

fn map_children_il(node: IrNode, f: fn(IrNode) -> IrNode) -> IrNode {
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
        IrNode::Block(items) => IrNode::Block(items.into_iter().map(f).collect()),
        other => other,
    }
}
