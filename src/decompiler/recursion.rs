use crate::ir::*;

/// Recognize Y-combinator / self-application patterns for recursion.
///
/// In UPLC, recursive functions are encoded using the Y-combinator or
/// self-application pattern:
///
/// ```
/// let f = fn(self) { fn(x) { ... self(self)(x) ... } }
/// f(f)(arg)
/// ```
///
/// We recognize this and convert to a named recursive function:
///
/// ```
/// fn f(x) {
///   ... f(x) ...
/// }
/// f(arg)
/// ```
pub fn recognize_recursion(node: IrNode) -> IrNode {
    match node {
        // Pattern: let f = fn(self) { body }; f(f)(args...)
        IrNode::LetBinding {
            name,
            value,
            body,
        } => {
            let value = recognize_recursion(*value);
            let body = recognize_recursion(*body);

            // Check if value is a lambda (the recursive function template)
            if let IrNode::Lambda {
                param_name: self_param,
                body: lambda_body,
            } = &value
            {
                // Check if body starts with self-application: name(name)(...)
                if let Some((inner_body, first_arg)) = extract_self_application(&body, &name) {
                    // Rewrite self(self)(x) calls to f(x) in the lambda body
                    let rewritten_body = rewrite_self_calls(
                        *lambda_body.clone(),
                        self_param,
                        &name,
                    );

                    return IrNode::Comment {
                        text: format!("// recursive fn {}", name),
                        node: Box::new(IrNode::LetBinding {
                            name: name.clone(),
                            value: Box::new(extract_inner_lambda(rewritten_body)),
                            body: Box::new(IrNode::FnCall {
                                function_name: name,
                                args: vec![first_arg],
                            }),
                        }),
                    };
                }
            }

            IrNode::LetBinding {
                name,
                value: Box::new(value),
                body: Box::new(body),
            }
        }
        _ => map_children_rec(node, recognize_recursion),
    }
}

/// Extract the argument from a self-application pattern: `f(f)(arg)` -> (inner, arg)
fn extract_self_application(node: &IrNode, name: &str) -> Option<(IrNode, IrNode)> {
    // Match: Apply(Apply(Var(f), Var(f)), arg) where both vars resolve to `name`
    // In our IR after scope-aware codegen, this might be:
    // Apply(Apply(Var(name), Var(name)), arg)
    // But since we use De Bruijn indices, it's more complex.
    // After name resolution in let-bindings, the pattern is:
    // Match { subject: Constr(0, [name_ref, name_ref, arg]), branches: [Var(1)] }
    // which was already converted to nested lets by constr_case_destruct.

    // For now, look for the simpler pattern after our passes:
    // Apply(FnCall or Apply involving name, arg)
    None // TODO: Implement full pattern matching
}

/// Rewrite self(self)(x) calls to f(x) calls in the body
fn rewrite_self_calls(node: IrNode, self_param: &str, fn_name: &str) -> IrNode {
    // TODO: Implement self-call rewriting
    node
}

/// Extract the inner lambda from fn(self) { fn(x) { body } }
fn extract_inner_lambda(node: IrNode) -> IrNode {
    match node {
        IrNode::Lambda { body, .. } => *body,
        other => other,
    }
}

fn map_children_rec(node: IrNode, f: fn(IrNode) -> IrNode) -> IrNode {
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
