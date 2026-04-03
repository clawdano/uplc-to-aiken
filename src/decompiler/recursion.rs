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
///
/// We also handle the Constr/Case-based fixpoint encoding:
///
/// ```
/// let f = fn(self) { fn(x1) { fn(x2) { ... when Constr_0(self, ...) is { Constr_0 -> self } ... } } }
/// when Constr_0(f, arg1, arg2) is { Constr_0 -> f }
/// ```
pub fn recognize_recursion(node: IrNode) -> IrNode {
    match node {
        // Pattern: let f = fn(self) { body }; <usage with self-application>
        IrNode::LetBinding {
            name,
            value,
            body,
        } => {
            let value = recognize_recursion(*value);
            let body = recognize_recursion(*body);

            // Check if value is a lambda (the recursive function template)
            if let IrNode::Lambda {
                param_name: ref self_param,
                body: ref lambda_body,
            } = value
            {
                // Count the inner params (nested lambdas after the self param)
                let (inner_params, innermost_body) = peel_lambdas(lambda_body);

                if !inner_params.is_empty() {
                    // Check for direct self-application in body: name(name)(args...)
                    if let Some(call_args) =
                        extract_self_application_call(&body, &name, inner_params.len())
                    {
                        // Check if the inner body contains self(self)(...) calls
                        // self_param is at De Bruijn depth = inner_params.len() + 1
                        // (counting from 1, where 1 = innermost lambda param)
                        let self_depth = inner_params.len() + 1;
                        if contains_self_application(&innermost_body, self_depth) {
                            let rewritten = rewrite_self_application(
                                innermost_body.clone(),
                                self_depth,
                                &name,
                                inner_params.len(),
                            );

                            // Build FnDef
                            let fn_def = IrNode::FnDef {
                                name: name.clone(),
                                params: inner_params.clone(),
                                body: Box::new(rewritten),
                            };

                            // Build the call site: name(arg1, arg2, ...)
                            let fn_call = IrNode::FnCall {
                                function_name: name.clone(),
                                args: call_args,
                            };

                            return IrNode::LetBinding {
                                name: format!("__fn_{}", name),
                                value: Box::new(fn_def),
                                body: Box::new(fn_call),
                            };
                        }
                    }

                    // Check for Constr/Case-based fixpoint in body:
                    // when Constr_0(name, arg1, arg2, ...) is { Constr_0 -> name }
                    if let Some(call_args) =
                        extract_constr_case_self_application(&body, &name, inner_params.len())
                    {
                        let self_depth = inner_params.len() + 1;
                        if contains_constr_self_application(&innermost_body, self_depth) {
                            let rewritten = rewrite_constr_self_application(
                                innermost_body.clone(),
                                self_depth,
                                &name,
                                inner_params.len(),
                            );

                            let fn_def = IrNode::FnDef {
                                name: name.clone(),
                                params: inner_params.clone(),
                                body: Box::new(rewritten),
                            };

                            let fn_call = IrNode::FnCall {
                                function_name: name.clone(),
                                args: call_args,
                            };

                            return IrNode::LetBinding {
                                name: format!("__fn_{}", name),
                                value: Box::new(fn_def),
                                body: Box::new(fn_call),
                            };
                        }
                    }
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

/// Peel off nested lambdas, returning param names and the innermost body.
fn peel_lambdas(node: &IrNode) -> (Vec<String>, &IrNode) {
    let mut params = Vec::new();
    let mut current = node;
    while let IrNode::Lambda { param_name, body } = current {
        params.push(param_name.clone());
        current = body;
    }
    (params, current)
}

// =============================================================================
// Direct self-application: f(f)(arg1)(arg2)...
// =============================================================================

/// Check if `body` is `Apply(...Apply(Apply(Var(1), Var(1)), arg1)..., argN)`
/// where Var(1) refers to the let-bound name.
/// Returns the list of arguments if it matches.
fn extract_self_application_call(
    body: &IrNode,
    _name: &str,
    expected_args: usize,
) -> Option<Vec<IrNode>> {
    // Peel off the outer Apply layers to collect arguments
    let mut args = Vec::new();
    let mut current = body;

    for _ in 0..expected_args {
        if let IrNode::Apply { function, argument } = current {
            args.push(*argument.clone());
            current = function;
        } else {
            return None;
        }
    }

    // Now current should be Apply(Var(1), Var(1))
    if let IrNode::Apply { function, argument } = current {
        if is_var_at_depth(&**function, 1) && is_var_at_depth(&**argument, 1) {
            args.reverse();
            return Some(args);
        }
    }

    None
}

/// Check if node is Var(depth)
fn is_var_at_depth(node: &IrNode, depth: usize) -> bool {
    matches!(node, IrNode::Var(d) if *d == depth)
}

/// Check if the body contains `Apply(Apply(Var(self_depth), Var(self_depth)), ...)`
fn contains_self_application(node: &IrNode, self_depth: usize) -> bool {
    match node {
        IrNode::Apply { function, argument } => {
            // Check if this is self(self)(...)
            if let IrNode::Apply {
                function: inner_fn,
                argument: inner_arg,
            } = &**function
            {
                if is_var_at_depth(inner_fn, self_depth)
                    && is_var_at_depth(inner_arg, self_depth)
                {
                    return true;
                }
            }
            contains_self_application(function, self_depth)
                || contains_self_application(argument, self_depth)
        }
        IrNode::Lambda { body, .. } => contains_self_application(body, self_depth + 1),
        IrNode::LetBinding { value, body, .. } => {
            contains_self_application(value, self_depth)
                || contains_self_application(body, self_depth + 1)
        }
        IrNode::IfElse {
            condition,
            then_branch,
            else_branch,
        } => {
            contains_self_application(condition, self_depth)
                || contains_self_application(then_branch, self_depth)
                || contains_self_application(else_branch, self_depth)
        }
        IrNode::BinOp { left, right, .. } => {
            contains_self_application(left, self_depth)
                || contains_self_application(right, self_depth)
        }
        IrNode::UnaryOp { operand, .. } => contains_self_application(operand, self_depth),
        IrNode::Force(inner) | IrNode::Delay(inner) => {
            contains_self_application(inner, self_depth)
        }
        IrNode::Trace { message, body } => {
            contains_self_application(message, self_depth)
                || contains_self_application(body, self_depth)
        }
        IrNode::Comment { node, .. } => contains_self_application(node, self_depth),
        IrNode::Match { subject, branches } => {
            contains_self_application(subject, self_depth)
                || branches
                    .iter()
                    .any(|b| contains_self_application(&b.body, self_depth))
        }
        IrNode::Constr { fields, .. } => fields
            .iter()
            .any(|f| contains_self_application(f, self_depth)),
        IrNode::FnCall { args, .. } => args
            .iter()
            .any(|a| contains_self_application(a, self_depth)),
        _ => false,
    }
}

/// Rewrite `Apply(Apply(Var(self_depth), Var(self_depth)), arg1, arg2, ...)`
/// to `FnCall { function_name, args: [arg1, arg2, ...] }`.
///
/// Also adjusts De Bruijn indices: since we're removing the outer lambda (self param),
/// all Var references above self_depth need to be decremented by 1.
fn rewrite_self_application(
    node: IrNode,
    self_depth: usize,
    fn_name: &str,
    num_params: usize,
) -> IrNode {
    match node {
        IrNode::Apply { function, argument } => {
            // Try to match self(self)(arg1)(arg2)...
            if let Some(args) =
                try_collect_self_app_args(&IrNode::Apply { function: function.clone(), argument: argument.clone() }, self_depth, num_params)
            {
                let rewritten_args = args
                    .into_iter()
                    .map(|a| rewrite_self_application(a, self_depth, fn_name, num_params))
                    .collect();
                return IrNode::FnCall {
                    function_name: fn_name.to_string(),
                    args: rewritten_args,
                };
            }

            // Not a self-application, recurse normally
            IrNode::Apply {
                function: Box::new(rewrite_self_application(
                    *function, self_depth, fn_name, num_params,
                )),
                argument: Box::new(rewrite_self_application(
                    *argument, self_depth, fn_name, num_params,
                )),
            }
        }

        // Adjust De Bruijn index: remove self param from scope
        IrNode::Var(idx) => {
            if idx == self_depth {
                // Direct reference to self (not as self-application) -- shouldn't happen
                // but keep it as a named reference
                IrNode::FnCall {
                    function_name: fn_name.to_string(),
                    args: vec![],
                }
            } else if idx > self_depth {
                // Reference past the self param -- decrement
                IrNode::Var(idx - 1)
            } else {
                IrNode::Var(idx)
            }
        }

        IrNode::Lambda { param_name, body } => IrNode::Lambda {
            param_name,
            body: Box::new(rewrite_self_application(
                *body,
                self_depth + 1,
                fn_name,
                num_params,
            )),
        },

        IrNode::LetBinding { name, value, body } => IrNode::LetBinding {
            name,
            value: Box::new(rewrite_self_application(
                *value, self_depth, fn_name, num_params,
            )),
            body: Box::new(rewrite_self_application(
                *body,
                self_depth + 1,
                fn_name,
                num_params,
            )),
        },

        IrNode::IfElse {
            condition,
            then_branch,
            else_branch,
        } => IrNode::IfElse {
            condition: Box::new(rewrite_self_application(
                *condition, self_depth, fn_name, num_params,
            )),
            then_branch: Box::new(rewrite_self_application(
                *then_branch, self_depth, fn_name, num_params,
            )),
            else_branch: Box::new(rewrite_self_application(
                *else_branch, self_depth, fn_name, num_params,
            )),
        },

        IrNode::BinOp { op, left, right } => IrNode::BinOp {
            op,
            left: Box::new(rewrite_self_application(
                *left, self_depth, fn_name, num_params,
            )),
            right: Box::new(rewrite_self_application(
                *right, self_depth, fn_name, num_params,
            )),
        },

        IrNode::UnaryOp { op, operand } => IrNode::UnaryOp {
            op,
            operand: Box::new(rewrite_self_application(
                *operand, self_depth, fn_name, num_params,
            )),
        },

        IrNode::Force(inner) => IrNode::Force(Box::new(rewrite_self_application(
            *inner, self_depth, fn_name, num_params,
        ))),

        IrNode::Delay(inner) => IrNode::Delay(Box::new(rewrite_self_application(
            *inner, self_depth, fn_name, num_params,
        ))),

        IrNode::Trace { message, body } => IrNode::Trace {
            message: Box::new(rewrite_self_application(
                *message, self_depth, fn_name, num_params,
            )),
            body: Box::new(rewrite_self_application(
                *body, self_depth, fn_name, num_params,
            )),
        },

        IrNode::Comment { text, node } => IrNode::Comment {
            text,
            node: Box::new(rewrite_self_application(
                *node, self_depth, fn_name, num_params,
            )),
        },

        IrNode::Match { subject, branches } => IrNode::Match {
            subject: Box::new(rewrite_self_application(
                *subject, self_depth, fn_name, num_params,
            )),
            branches: branches
                .into_iter()
                .map(|b| MatchBranch {
                    pattern: b.pattern,
                    body: rewrite_self_application(b.body, self_depth, fn_name, num_params),
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
                .map(|f| rewrite_self_application(f, self_depth, fn_name, num_params))
                .collect(),
        },

        IrNode::FnCall {
            function_name,
            args,
        } => IrNode::FnCall {
            function_name,
            args: args
                .into_iter()
                .map(|a| rewrite_self_application(a, self_depth, fn_name, num_params))
                .collect(),
        },

        IrNode::FieldAccess {
            record,
            field_index,
            field_name,
        } => IrNode::FieldAccess {
            record: Box::new(rewrite_self_application(
                *record, self_depth, fn_name, num_params,
            )),
            field_index,
            field_name,
        },

        IrNode::Block(items) => IrNode::Block(
            items
                .into_iter()
                .map(|i| rewrite_self_application(i, self_depth, fn_name, num_params))
                .collect(),
        ),

        IrNode::Expect {
            pattern,
            value,
            body,
        } => IrNode::Expect {
            pattern: Box::new(rewrite_self_application(
                *pattern, self_depth, fn_name, num_params,
            )),
            value: Box::new(rewrite_self_application(
                *value, self_depth, fn_name, num_params,
            )),
            body: Box::new(rewrite_self_application(
                *body, self_depth, fn_name, num_params,
            )),
        },

        // Leaf nodes
        other => other,
    }
}

/// Try to collect N arguments from a chain of Apply nodes where the core is
/// Apply(Var(self_depth), Var(self_depth)).
/// Returns Some(args) if matched, None otherwise.
fn try_collect_self_app_args(
    node: &IrNode,
    self_depth: usize,
    num_params: usize,
) -> Option<Vec<IrNode>> {
    let mut args = Vec::new();
    let mut current = node;

    for _ in 0..num_params {
        if let IrNode::Apply { function, argument } = current {
            args.push(*argument.clone());
            current = function;
        } else {
            return None;
        }
    }

    // Now current should be Apply(Var(self_depth), Var(self_depth))
    if let IrNode::Apply { function, argument } = current {
        if is_var_at_depth(function, self_depth) && is_var_at_depth(argument, self_depth) {
            args.reverse();
            return Some(args);
        }
    }

    None
}

// =============================================================================
// Constr/Case-based fixpoint: when Constr_0(f, args...) is { Constr_0 -> f }
// =============================================================================

/// Check if `body` is a Constr/Case fixpoint call:
/// `Match { subject: Constr { tag: 0, fields: [Var(1), arg1, arg2, ...] }, branches: [Branch { body: Var(n) }] }`
/// where Var(1) refers to the let-bound name and Var(n) in the branch refers to the function.
fn extract_constr_case_self_application(
    body: &IrNode,
    _name: &str,
    expected_args: usize,
) -> Option<Vec<IrNode>> {
    if let IrNode::Match { subject, branches } = body {
        if let IrNode::Constr { tag: 0, fields, .. } = &**subject {
            // First field should be Var(1) (the self-reference)
            if fields.len() >= 1 + expected_args && is_var_at_depth(&fields[0], 1) {
                // The branch should contain just a Var that refers to the first field
                // (the function extracted from the constr)
                if branches.len() == 1 {
                    if let IrNode::Var(branch_var) = &branches[0].body {
                        // In the branch, after destructuring Constr_0(f, a1, a2, ...),
                        // Var(1) = last field, Var(n) = first field (the function)
                        // Actually the branch body Var should refer to the function field
                        let total_fields = fields.len();
                        if *branch_var == total_fields {
                            let args: Vec<IrNode> =
                                fields[1..].iter().cloned().collect();
                            return Some(args);
                        }
                    }
                }
            }
        }
    }
    None
}

/// Check if the body contains a Constr/Case self-application pattern:
/// `Match { subject: Constr { tag: 0, fields: [Var(self_depth), ...] }, branches: [Var(n)] }`
fn contains_constr_self_application(node: &IrNode, self_depth: usize) -> bool {
    match node {
        IrNode::Match { subject, branches } => {
            if let IrNode::Constr { tag: 0, fields, .. } = &**subject {
                if !fields.is_empty() && is_var_at_depth(&fields[0], self_depth) {
                    if branches.len() == 1 {
                        if let IrNode::Var(v) = &branches[0].body {
                            if *v == fields.len() {
                                return true;
                            }
                        }
                    }
                }
            }
            // Still recurse into children
            contains_constr_self_application_children(node, self_depth)
        }
        _ => contains_constr_self_application_children(node, self_depth),
    }
}

fn contains_constr_self_application_children(node: &IrNode, self_depth: usize) -> bool {
    match node {
        IrNode::Apply { function, argument } => {
            contains_constr_self_application(function, self_depth)
                || contains_constr_self_application(argument, self_depth)
        }
        IrNode::Lambda { body, .. } => contains_constr_self_application(body, self_depth + 1),
        IrNode::LetBinding { value, body, .. } => {
            contains_constr_self_application(value, self_depth)
                || contains_constr_self_application(body, self_depth + 1)
        }
        IrNode::IfElse {
            condition,
            then_branch,
            else_branch,
        } => {
            contains_constr_self_application(condition, self_depth)
                || contains_constr_self_application(then_branch, self_depth)
                || contains_constr_self_application(else_branch, self_depth)
        }
        IrNode::BinOp { left, right, .. } => {
            contains_constr_self_application(left, self_depth)
                || contains_constr_self_application(right, self_depth)
        }
        IrNode::UnaryOp { operand, .. } => contains_constr_self_application(operand, self_depth),
        IrNode::Force(inner) | IrNode::Delay(inner) => {
            contains_constr_self_application(inner, self_depth)
        }
        IrNode::Trace { message, body } => {
            contains_constr_self_application(message, self_depth)
                || contains_constr_self_application(body, self_depth)
        }
        IrNode::Comment { node, .. } => contains_constr_self_application(node, self_depth),
        IrNode::Match { subject, branches } => {
            contains_constr_self_application(subject, self_depth)
                || branches
                    .iter()
                    .any(|b| contains_constr_self_application(&b.body, self_depth))
        }
        IrNode::Constr { fields, .. } => fields
            .iter()
            .any(|f| contains_constr_self_application(f, self_depth)),
        IrNode::FnCall { args, .. } => args
            .iter()
            .any(|a| contains_constr_self_application(a, self_depth)),
        _ => false,
    }
}

/// Rewrite Constr/Case self-application patterns to FnCall.
/// The pattern: `Match { subject: Constr(0, [Var(self_depth), arg1, arg2, ...]), branches: [Var(n)] }`
/// becomes: `FnCall { function_name, args: [arg1, arg2, ...] }`
fn rewrite_constr_self_application(
    node: IrNode,
    self_depth: usize,
    fn_name: &str,
    num_params: usize,
) -> IrNode {
    match node {
        IrNode::Match { subject, branches } => {
            if let IrNode::Constr { tag: 0, fields, .. } = &*subject {
                if !fields.is_empty() && is_var_at_depth(&fields[0], self_depth) {
                    if branches.len() == 1 {
                        if let IrNode::Var(v) = &branches[0].body {
                            if *v == fields.len() {
                                // This is a self-application via Constr/Case
                                let args: Vec<IrNode> = fields[1..]
                                    .iter()
                                    .map(|a| {
                                        rewrite_constr_self_application(
                                            a.clone(),
                                            self_depth,
                                            fn_name,
                                            num_params,
                                        )
                                    })
                                    .collect();
                                return IrNode::FnCall {
                                    function_name: fn_name.to_string(),
                                    args,
                                };
                            }
                        }
                    }
                }
            }
            // Not a self-application match, recurse
            IrNode::Match {
                subject: Box::new(rewrite_constr_self_application(
                    *subject, self_depth, fn_name, num_params,
                )),
                branches: branches
                    .into_iter()
                    .map(|b| MatchBranch {
                        pattern: b.pattern,
                        body: rewrite_constr_self_application(b.body, self_depth, fn_name, num_params),
                    })
                    .collect(),
            }
        }

        // Adjust De Bruijn index: remove self param from scope
        IrNode::Var(idx) => {
            if idx == self_depth {
                IrNode::FnCall {
                    function_name: fn_name.to_string(),
                    args: vec![],
                }
            } else if idx > self_depth {
                IrNode::Var(idx - 1)
            } else {
                IrNode::Var(idx)
            }
        }

        IrNode::Lambda { param_name, body } => IrNode::Lambda {
            param_name,
            body: Box::new(rewrite_constr_self_application(
                *body,
                self_depth + 1,
                fn_name,
                num_params,
            )),
        },

        IrNode::LetBinding { name, value, body } => IrNode::LetBinding {
            name,
            value: Box::new(rewrite_constr_self_application(
                *value, self_depth, fn_name, num_params,
            )),
            body: Box::new(rewrite_constr_self_application(
                *body,
                self_depth + 1,
                fn_name,
                num_params,
            )),
        },

        IrNode::IfElse {
            condition,
            then_branch,
            else_branch,
        } => IrNode::IfElse {
            condition: Box::new(rewrite_constr_self_application(
                *condition, self_depth, fn_name, num_params,
            )),
            then_branch: Box::new(rewrite_constr_self_application(
                *then_branch, self_depth, fn_name, num_params,
            )),
            else_branch: Box::new(rewrite_constr_self_application(
                *else_branch, self_depth, fn_name, num_params,
            )),
        },

        IrNode::BinOp { op, left, right } => IrNode::BinOp {
            op,
            left: Box::new(rewrite_constr_self_application(
                *left, self_depth, fn_name, num_params,
            )),
            right: Box::new(rewrite_constr_self_application(
                *right, self_depth, fn_name, num_params,
            )),
        },

        IrNode::UnaryOp { op, operand } => IrNode::UnaryOp {
            op,
            operand: Box::new(rewrite_constr_self_application(
                *operand, self_depth, fn_name, num_params,
            )),
        },

        IrNode::Force(inner) => IrNode::Force(Box::new(rewrite_constr_self_application(
            *inner, self_depth, fn_name, num_params,
        ))),

        IrNode::Delay(inner) => IrNode::Delay(Box::new(rewrite_constr_self_application(
            *inner, self_depth, fn_name, num_params,
        ))),

        IrNode::Trace { message, body } => IrNode::Trace {
            message: Box::new(rewrite_constr_self_application(
                *message, self_depth, fn_name, num_params,
            )),
            body: Box::new(rewrite_constr_self_application(
                *body, self_depth, fn_name, num_params,
            )),
        },

        IrNode::Comment { text, node } => IrNode::Comment {
            text,
            node: Box::new(rewrite_constr_self_application(
                *node, self_depth, fn_name, num_params,
            )),
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
                .map(|f| rewrite_constr_self_application(f, self_depth, fn_name, num_params))
                .collect(),
        },

        IrNode::FnCall {
            function_name,
            args,
        } => IrNode::FnCall {
            function_name,
            args: args
                .into_iter()
                .map(|a| rewrite_constr_self_application(a, self_depth, fn_name, num_params))
                .collect(),
        },

        IrNode::FieldAccess {
            record,
            field_index,
            field_name,
        } => IrNode::FieldAccess {
            record: Box::new(rewrite_constr_self_application(
                *record, self_depth, fn_name, num_params,
            )),
            field_index,
            field_name,
        },

        IrNode::Apply { function, argument } => IrNode::Apply {
            function: Box::new(rewrite_constr_self_application(
                *function, self_depth, fn_name, num_params,
            )),
            argument: Box::new(rewrite_constr_self_application(
                *argument, self_depth, fn_name, num_params,
            )),
        },

        IrNode::Block(items) => IrNode::Block(
            items
                .into_iter()
                .map(|i| rewrite_constr_self_application(i, self_depth, fn_name, num_params))
                .collect(),
        ),

        IrNode::Expect {
            pattern,
            value,
            body,
        } => IrNode::Expect {
            pattern: Box::new(rewrite_constr_self_application(
                *pattern, self_depth, fn_name, num_params,
            )),
            value: Box::new(rewrite_constr_self_application(
                *value, self_depth, fn_name, num_params,
            )),
            body: Box::new(rewrite_constr_self_application(
                *body, self_depth, fn_name, num_params,
            )),
        },

        // Leaf nodes
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
        IrNode::FnDef { name, params, body } => IrNode::FnDef {
            name,
            params,
            body: Box::new(f(*body)),
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
        other => other,
    }
}
