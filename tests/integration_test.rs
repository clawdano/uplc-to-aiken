use std::process::Command;

fn decompile_fixture(name: &str) -> String {
    let output = Command::new("cargo")
        .args(["run", "--quiet", "--", "decompile", "--input"])
        .arg(format!("tests/fixtures/{}/script.cbor.hex", name))
        .output()
        .expect("Failed to execute decompiler");

    String::from_utf8(output.stdout).unwrap()
}

#[test]
fn always_true_contains_true() {
    let output = decompile_fixture("always_true");
    assert!(output.contains("True"), "Expected 'True' in output:\n{}", output);
}

#[test]
fn check_42_contains_equality_check() {
    let output = decompile_fixture("check_42");
    assert!(output.contains("== 42"), "Expected '== 42' in output:\n{}", output);
}

#[test]
fn math_check_contains_arithmetic() {
    let output = decompile_fixture("math_check");
    assert!(output.contains("* 2"), "Expected '* 2' in output:\n{}", output);
    assert!(output.contains("== 84"), "Expected '== 84' in output:\n{}", output);
    assert!(output.contains("0 <"), "Expected '0 <' in output:\n{}", output);
    // The `if a { b } else { False }` pattern is now recognized as `a && b`
    assert!(
        output.contains("&&") || output.contains("False"),
        "Expected '&&' or 'False' in output:\n{}",
        output
    );
}

#[test]
fn multi_condition_contains_logic() {
    let output = decompile_fixture("multi_condition");
    assert!(output.contains("* 2"), "Expected '* 2' in output:\n{}", output);
    assert!(output.contains("< 100"), "Expected '< 100' in output:\n{}", output);
}

#[test]
fn with_helper_shows_inlined_function() {
    let output = decompile_fixture("with_helper");
    assert!(output.contains("+ 10"), "Expected '+ 10' in output:\n{}", output);
    assert!(output.contains("42"), "Expected '42' in output:\n{}", output);
}

#[test]
fn no_builtin_pack_in_output() {
    // Verify that builtin let-bindings are stripped from all outputs
    for fixture in ["always_true", "check_42", "math_check", "multi_condition"] {
        let output = decompile_fixture(fixture);
        assert!(
            !output.contains("let tail_list"),
            "Builtin pack not stripped in {}: {}",
            fixture,
            output
        );
        assert!(
            !output.contains("let head_list"),
            "Builtin pack not stripped in {}: {}",
            fixture,
            output
        );
    }
}

#[test]
fn recursive_fns_shows_recursion_pattern() {
    let output = decompile_fixture("recursive_fns");
    // Fibonacci: base case and recursive structure
    assert!(output.contains("<= 0"), "Expected '<= 0' in output:\n{}", output);
    // Recursive subtraction pattern (n + -1 or n + -2)
    assert!(
        output.contains("+ -1") || output.contains("+ -2"),
        "Expected recursive subtraction pattern in output:\n{}",
        output
    );
}

#[test]
fn dex_swap_decompiles_without_error() {
    let output = decompile_fixture("dex_swap");
    assert!(!output.is_empty(), "Expected non-empty output for dex_swap");
    assert!(output.len() > 100, "Expected substantial output for complex contract");
}

#[test]
fn all_fixtures_decompile_successfully() {
    for fixture in [
        "always_true", "check_42", "math_check", "multi_condition",
        "with_helper", "traced", "token_policy", "option_check",
        "dex_swap", "hash_ops", "list_ops", "nested_pattern",
        "recursive_fns", "token_minter", "tx_info_check",
    ] {
        let output = decompile_fixture(fixture);
        assert!(
            !output.is_empty(),
            "Fixture {} produced empty output",
            fixture
        );
    }
}

#[test]
fn show_ast_flag_works() {
    let output = Command::new("cargo")
        .args([
            "run", "--quiet", "--", "decompile", "--input",
            "tests/fixtures/always_true/script.cbor.hex",
            "--show-ast",
        ])
        .output()
        .expect("Failed to execute");

    let stdout = String::from_utf8(output.stdout).unwrap();
    assert!(stdout.contains("Program"), "Expected AST output");
    assert!(stdout.contains("Lambda"), "Expected Lambda in AST");
}

#[test]
fn show_ir_flag_works() {
    let output = Command::new("cargo")
        .args([
            "run", "--quiet", "--", "decompile", "--input",
            "tests/fixtures/always_true/script.cbor.hex",
            "--show-ir",
        ])
        .output()
        .expect("Failed to execute");

    let stdout = String::from_utf8(output.stdout).unwrap();
    assert!(stdout.contains("Lambda"), "Expected IR output with Lambda");
}
