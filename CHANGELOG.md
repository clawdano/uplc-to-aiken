# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added
- Initial project scaffolding with Rust/Cargo
- UPLC parser supporting both CBOR hex and text format input
- Four-phase decompilation pipeline: Parse -> Lower to IR -> Pattern Recognition -> Aiken Codegen
- CLI interface with `decompile` command, `--show-ast`, `--show-ir` debug flags
- Plutus V3 pattern recognition:
  - Builtin pack unpacking (Aiken's optimization for frequently-used builtins)
  - V3 if-then-else recognition (Constr/Case pattern)
  - Constr/Case destructuring to let-bindings
- Standard pattern recognition passes:
  - If-then-else from IfThenElse builtin applications
  - Binary operations (arithmetic, comparison, equality, append)
  - Let-binding recognition from lambda applications
  - Trace builtin recognition
  - Bool literal recognition (Constr(0,[]) = False, Constr(1,[]) = True)
  - Unit/Void recognition
  - List operations (head, tail, null)
  - Data deconstruction annotations (UnIData, UnBData, etc.)
  - Logical operators (`&&` from `if a { b } else { False }`, `||` from `if a { True } else { b }`)
- Validator wrapper recognition:
  - Strip outermost multi-validator dispatch lambda
  - Strip implementation-detail builtin let-bindings
  - Strip multi-validator dispatch wrapper (`if body { Void } else { fail }`)
  - De Bruijn index shifting when removing binders
- Scope-aware codegen with De Bruijn variable resolution
- Variable name synthesis based on usage context
- Constant simplification (Integer/ByteString/String/Bool constants to Aiken literals)
- 8 integration tests covering core functionality
- 7 test fixtures from compiled Aiken contracts (always_true, check_42, math_check, multi_condition, with_helper, traced, token_policy)
- Apache-2.0 license
- README, CONTRIBUTING guide
