# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added
- Seven-phase decompilation pipeline: V3 Structural -> Basic Patterns -> Validator Wrapper -> Post-Inlining -> Aiken Patterns -> High-Level Sugar -> Name Assignment
- UPLC parser supporting CBOR hex (on-chain format) and text-format UPLC input
- CLI with `decompile` command, `--show-ast`, `--show-ir` debug flags, `--hex` direct input
- Plutus V3 pattern recognition:
  - Builtin pack unpacking (Aiken's optimization for frequently-used builtins)
  - V3 if-then-else recognition (Constr/Case pattern)
  - Constr/Case destructuring to let-bindings
- Builtin inlining: substitutes builtin values at usage sites (preserves correct semantics vs stripping)
- Aiken-specific compilation pattern recognition:
  - CONSTR_FIELDS_EXPOSER: `sndPair(unConstrData(x))` -> `constr_fields(x)`
  - CONSTR_INDEX_EXPOSER: `fstPair(unConstrData(x))` -> `constr_index(x)`
  - Field access: `headList(tailList^n(fields))` -> `x.field_n`
  - Constructor tag checks with type hints
  - Datum Some/None check pattern
- Standard pattern recognition passes:
  - If-then-else, binary operations (arithmetic, comparison, equality, append)
  - Let-binding recognition (lambda applications -> let bindings)
  - Trace, Bool literals, Unit/Void, list operations
  - Data deconstruction annotations (`// expect: Int`, `// expect: ByteArray`)
  - Logical operators (`&&` / `||`)
  - Crypto/hash operations (blake2b_256, sha2_256, length_of_bytearray)
- Validator wrapper cleanup:
  - Inline builtin let-bindings with proper De Bruijn substitution
  - Strip outermost multi-validator dispatch lambda
  - Strip dispatch wrapper (`if body { Void } else { fail }`)
- Smart codegen:
  - Scope-aware De Bruijn variable resolution
  - Detect Apply(LetBinding{..., Lambda}, arg) -> let-binding conversion
  - Context-aware variable name synthesis (fields/tag/n/bytes/result/etc.)
- 15 test fixtures from compiled Aiken contracts:
  - Simple: always_true, check_42, math_check, multi_condition, with_helper, traced, option_check
  - Complex: token_policy, dex_swap (322 lines), hash_ops, list_ops, nested_pattern, recursive_fns, token_minter, tx_info_check
- 11 integration tests including all-fixtures-decompile-successfully
- Apache-2.0 license, README, CONTRIBUTING guide, CHANGELOG
