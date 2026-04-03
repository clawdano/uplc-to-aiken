# Contributing to uplc-to-aiken

Thank you for your interest in contributing!

## Getting Started

1. Fork the repository
2. Clone your fork: `git clone git@github.com:YOUR_USERNAME/uplc-to-aiken.git`
3. Create a branch: `git checkout -b my-feature`
4. Make your changes
5. Run tests: `cargo test`
6. Push and open a PR

## Development

```bash
# Build
cargo build

# Run tests
cargo test

# Run with a UPLC file
cargo run -- decompile --input script.uplc

# Run with CBOR hex
cargo run -- decompile --hex 5901234...
```

## Adding Test Cases

The best way to contribute is by adding test cases - Aiken contracts paired with their compiled UPLC output. Place them in `tests/fixtures/`:

```
tests/fixtures/
  my_contract/
    source.ak          # Original Aiken source (if known)
    script.uplc        # Text-format UPLC
    script.cbor.hex    # CBOR hex encoding
    expected.ak        # Expected decompiler output
```

## Code of Conduct

Be kind. Be constructive. We're all here to learn and build cool things on Cardano.
