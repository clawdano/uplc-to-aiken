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
/// leaving just the core validator logic.
pub fn recognize_validator(node: IrNode) -> IrNode {
    match node {
        IrNode::Lambda { body, .. } => {
            // Strip the outermost lambda (multi-validator dispatch)
            strip_builtin_lets(*body)
        }
        other => other,
    }
}

/// Strip the builtin let-bindings that are Aiken implementation details.
/// These are the forced builtin functions that Aiken packs for efficiency.
fn strip_builtin_lets(node: IrNode) -> IrNode {
    match node {
        IrNode::LetBinding { ref name, ref body, .. }
            if is_builtin_let_name(name) =>
        {
            strip_builtin_lets(*body.clone())
        }
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
