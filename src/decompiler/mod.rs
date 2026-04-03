mod names;
mod passes;
mod v3_patterns;
mod validator;

use crate::ir::IrNode;

/// Run all decompilation passes on the IR to recognize patterns
/// and produce higher-level Aiken constructs.
pub fn decompile(ir: IrNode) -> IrNode {
    // V3 patterns first - they operate on the raw Constr/Case structure
    let ir = v3_patterns::unpack_builtin_pack(ir);
    let ir = v3_patterns::recognize_v3_if_then_else(ir);
    let ir = v3_patterns::recognize_constr_case_destruct(ir);

    // Standard pattern recognition
    let ir = passes::recognize_if_then_else(ir);
    let ir = passes::recognize_let_bindings(ir);
    let ir = passes::recognize_trace(ir);
    let ir = passes::recognize_bool_literals(ir);
    let ir = passes::recognize_unit(ir);
    let ir = passes::recognize_list_ops(ir);
    let ir = passes::recognize_data_deconstruction(ir);
    // Binops after let-binding recognition so curried builtins are visible
    let ir = passes::recognize_binops(ir);

    // Validator wrapper recognition (after other passes clean up the structure)
    let ir = validator::recognize_validator(ir);

    let ir = names::assign_names(ir);
    ir
}
