mod aiken_patterns;
mod inline;
mod names;
mod passes;
mod pattern_match;
mod recursion;
mod v3_patterns;
mod validator;

use crate::ir::IrNode;

/// Run all decompilation passes on the IR to recognize patterns
/// and produce higher-level Aiken constructs.
pub fn decompile(ir: IrNode) -> IrNode {
    // Phase 1: V3 structural patterns
    let ir = v3_patterns::unpack_builtin_pack(ir);
    let ir = v3_patterns::recognize_v3_if_then_else(ir);
    let ir = v3_patterns::recognize_constr_case_destruct(ir);

    // Phase 2: Basic pattern recognition (before validator stripping)
    let ir = passes::recognize_if_then_else(ir);
    let ir = passes::recognize_let_bindings(ir);
    let ir = passes::recognize_trace(ir);
    let ir = passes::recognize_bool_literals(ir);
    let ir = passes::recognize_unit(ir);
    let ir = passes::recognize_binops(ir);

    // Phase 3: Validator wrapper (inlines builtins, strips wrapper)
    let ir = validator::recognize_validator(ir);

    // Phase 4: Re-run passes that benefit from inlined builtins
    let ir = passes::recognize_let_bindings(ir);
    let ir = passes::recognize_list_ops(ir);
    let ir = passes::recognize_data_deconstruction(ir);
    let ir = passes::recognize_binops(ir);

    // Phase 5: Aiken-specific patterns
    let ir = aiken_patterns::recognize_aiken_patterns(ir);

    // Phase 5.5: Inline simple partial-application lets to expose patterns
    let ir = inline::inline_simple_lets(ir);
    // Re-run binops after inlining (catches equals_integer(N)(x) -> N == x)
    let ir = passes::recognize_binops(ir);

    // Phase 6: Pattern matching recognition
    let ir = pattern_match::recognize_pattern_matching(ir);

    // Phase 7: High-level sugar
    let ir = passes::recognize_list_ops(ir);
    let ir = passes::recognize_logical_ops(ir);
    let ir = passes::simplify_constants(ir);
    let ir = passes::recognize_let_bindings(ir);
    let ir = passes::recognize_binops(ir);

    // Phase 7.5: Recursion recognition (Y-combinator / self-application)
    let ir = recursion::recognize_recursion(ir);

    // Phase 8: Name assignment
    let ir = names::assign_names(ir);
    ir
}
