mod names;
mod passes;

use crate::ir::IrNode;

/// Run all decompilation passes on the IR to recognize patterns
/// and produce higher-level Aiken constructs.
pub fn decompile(ir: IrNode) -> IrNode {
    let ir = passes::recognize_if_then_else(ir);
    let ir = passes::recognize_binops(ir);
    let ir = passes::recognize_let_bindings(ir);
    let ir = passes::recognize_trace(ir);
    let ir = passes::recognize_bool_literals(ir);
    let ir = passes::recognize_unit(ir);
    let ir = passes::recognize_list_ops(ir);
    let ir = passes::recognize_data_deconstruction(ir);
    let ir = names::assign_names(ir);
    ir
}
