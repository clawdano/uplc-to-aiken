# uplc-to-aiken

A UPLC bytecode decompiler targeting Aiken - reverse-engineer Cardano smart contracts into readable Aiken code.

## What it does

`uplc-to-aiken` takes compiled Plutus UPLC bytecode (the format smart contracts live on-chain in) and produces human-readable Aiken source code. This is useful for:

- **Auditing on-chain contracts** - understand what a deployed script actually does
- **Debugging your own contracts** - see how your Aiken code compiles and trace bugs
- **Learning** - understand the relationship between high-level Aiken and low-level UPLC

> **Note**: The decompiled output is a best-effort reconstruction. Variable names, type information, and some structural sugar are lost during compilation. The decompiler adds comments where it can't perfectly reconstruct the original code.

## Installation

```bash
cargo install uplc-to-aiken
```

Or build from source:

```bash
git clone https://github.com/clawdano/uplc-to-aiken.git
cd uplc-to-aiken
cargo build --release
```

## Usage

### From CBOR hex (on-chain format)

```bash
# From a file containing CBOR hex
uplc-to-aiken decompile --input script.cbor.hex

# Directly from a hex string
uplc-to-aiken decompile --hex "585c01010029800aba2..."

# Save to file
uplc-to-aiken decompile --input script.cbor.hex --output decompiled.ak
```

### From text-format UPLC

```bash
uplc-to-aiken decompile --input script.uplc
```

### Debug options

```bash
# Show the raw UPLC AST
uplc-to-aiken decompile --input script.cbor.hex --show-ast

# Show the intermediate representation
uplc-to-aiken decompile --input script.cbor.hex --show-ir
```

## Architecture

The decompiler works in four phases:

```
CBOR/Text UPLC  -->  Parse  -->  UPLC AST
                                    |
                                    v
                              Lower to IR
                                    |
                                    v
                         Pattern Recognition
                          (decompiler passes)
                                    |
                                    v
                           Aiken Code Generation
```

### Decompilation passes

1. **If-then-else recognition** - Converts `force (force ifThenElse) cond (delay t) (delay f)` to `if cond { t } else { f }`
2. **Binary operation recognition** - Converts builtin applications like `addInteger a b` to `a + b`
3. **Let binding recognition** - Converts `(\x -> body) value` to `let x = value; body`
4. **Trace recognition** - Converts trace builtin calls to `trace "msg"`
5. **Bool literal recognition** - Converts `Constr(0, [])` / `Constr(1, [])` to `False` / `True`
6. **List operation recognition** - Converts head/tail/null builtins to Aiken equivalents
7. **Data deconstruction** - Annotates `UnIData`, `UnBData` etc. with type comments
8. **Name assignment** - Synthesizes meaningful variable names based on usage context

## Supported features

| Feature | Status |
|---------|--------|
| Plutus V2 input | Supported |
| Plutus V3 input | Planned |
| CBOR hex input | Supported |
| Text UPLC input | Supported |
| If/else | Supported |
| Arithmetic operators | Supported |
| Comparison operators | Supported |
| Let bindings | Supported |
| Trace | Supported |
| Bool literals | Supported |
| Pattern matching (when/is) | Partial |
| Custom types | Planned |
| Validator detection | Planned |
| Function extraction | Planned |
| List comprehensions | Planned |
| Standard library mapping | Planned |

## Limitations

- **Variable names are synthesized** - UPLC uses De Bruijn indices; original names are lost
- **Type information is lost** - The decompiler infers types where possible but can't fully reconstruct them
- **Code structure may differ** - The decompiled output may not match the original source structure
- **Recompilation** - The output generally won't recompile to the same bytecode

## Contributing

See [CONTRIBUTING.md](CONTRIBUTING.md) for details.

## License

Apache-2.0 - see [LICENSE](LICENSE)
