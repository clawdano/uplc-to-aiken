# UPLC (Untyped Plutus Lambda Calculus) Research for Decompiler

## Table of Contents
1. [UPLC AST Structure](#1-uplc-ast-structure)
2. [CBOR/Flat Encoding](#2-cborflat-encoding)
3. [Plutus V2 Builtins](#3-plutus-v2-builtins)
4. [Plutus V2 vs V3 Differences](#4-plutus-v2-vs-v3-differences)
5. [Existing Rust Crates](#5-existing-rust-crates)
6. [The Data Type](#6-the-data-type)

---

## 1. UPLC AST Structure

UPLC is an eagerly-evaluated untyped lambda calculus extended with built-in types and functions. It is the lowest-level representation of Cardano smart contracts -- what actually executes on-chain.

### Program Wrapper

Every UPLC program is wrapped in a `Program` that carries a version number:

```
Program = (version_major, version_minor, version_patch, Term)
```

- Plutus V1/V2 use Plutus Core version **1.0.0**
- Plutus V3 uses Plutus Core version **1.1.0** (adds Constr/Case)

### Term Node Types

The UPLC `Term` has **10 variants** (8 original + 2 added in v1.1.0):

#### Original 8 (Plutus Core 1.0.0):

| Tag | Name     | Structure                    | Description |
|-----|----------|------------------------------|-------------|
| 0   | **Var**      | `Var(DeBruijn)`              | Variable reference using de Bruijn index |
| 1   | **Delay**    | `Delay(Term)`                | Defers evaluation of a term (laziness) |
| 2   | **Lambda**   | `Lambda { param, body: Term }`| Lambda abstraction (function definition) |
| 3   | **Apply**    | `Apply { function: Term, argument: Term }` | Function application |
| 4   | **Constant** | `Constant(Value)`            | Literal constant value |
| 5   | **Force**    | `Force(Term)`                | Forces evaluation of a delayed term |
| 6   | **Error**    | `Error`                      | Unconditional failure |
| 7   | **Builtin**  | `Builtin(DefaultFunction)`   | Reference to a built-in function |

#### Added in Plutus Core 1.1.0 (V3):

| Tag | Name     | Structure                    | Description |
|-----|----------|------------------------------|-------------|
| 8   | **Constr**   | `Constr { tag: usize, fields: Vec<Term> }` | Constructor (sum-of-products) |
| 9   | **Case**     | `Case { constr: Term, branches: Vec<Term> }` | Pattern match on constructor |

### De Bruijn Indices

Variables use **de Bruijn indices** instead of names. A `Var(n)` refers to "the variable bound by the n-th enclosing lambda abstraction," counting outward from 1.

Example: The identity function `\x -> x` is `Lambda { body: Var(1) }` because `x` is bound by the 1st (innermost) lambda.

Example: `\x -> \y -> x` is `Lambda { body: Lambda { body: Var(2) } }` because `x` is bound by the 2nd enclosing lambda.

### Constant Types (DefaultUni)

Constants can hold values of these built-in types:

| Type Tag | Type                  | Rust Representation |
|----------|-----------------------|---------------------|
| 0        | `integer`             | `BigInt` (arbitrary precision) |
| 1        | `bytestring`          | `Vec<u8>` |
| 2        | `string`              | `String` (UTF-8) |
| 3        | `unit`                | `()` |
| 4        | `bool`                | `bool` |
| 7,5,...  | `list<T>`             | `Vec<Constant>` + element type |
| 7,7,6,...| `pair<A,B>`           | `(Constant, Constant)` + both types |
| 8        | `data`                | `PlutusData` (CBOR-encoded) |
| 9        | `bls12_381_G1_element` | BLS curve point (V3 only) |
| 10       | `bls12_381_G2_element` | BLS curve point (V3 only) |
| 11       | `bls12_381_mlresult`  | BLS pairing result (V3 only) |

### Force/Delay Mechanism

`Force` and `Delay` implement type-level polymorphism at the term level. Polymorphic builtins require `Force` applications to instantiate their type parameters before use:

- **0 Forces**: Monomorphic functions like `AddInteger`
- **1 Force**: Functions polymorphic in 1 type, like `IfThenElse`, `HeadList`, `Trace`
- **2 Forces**: Functions polymorphic in 2 types, like `FstPair`, `ChooseList`

Example: To use `IfThenElse`, you write:
```
(force (builtin ifThenElse))
```
Then apply it: `[[[force (builtin ifThenElse)] condition] then_branch] else_branch]`

---

## 2. CBOR/Flat Encoding

On-chain Plutus scripts use a **two-layer encoding**: the UPLC AST is encoded in **flat** binary format, then the flat bytes are wrapped in **CBOR** as a bytestring.

### Layer 1: Flat Binary Encoding

Flat is a bit-level (not byte-level) encoding optimized for compactness. It is approximately 35% smaller than CBOR for UPLC programs.

#### Program Encoding

```
Encode(program a.b.c t) = Pad(EncodeTerm(EncodeNat(EncodeNat(EncodeNat(empty, a), b), c), t))
```

The version numbers are encoded as variable-length natural numbers, followed by the term, then padding.

#### Padding

After encoding, the bit stream is padded to a byte boundary by appending zeros followed by a 1:
- If already byte-aligned: append `00000001` (full byte of padding)
- If 1 bit past: append `0000001`
- If 2 bits past: append `000001`
- ... and so on up to 7 bits past: append `1`

This ensures the decoder can distinguish padding from data.

#### Term Encoding (4-bit tags)

Each term starts with a 4-bit tag:

```
Var:      0000 + EncodeName(index)
Delay:    0001 + EncodeTerm(body)
Lambda:   0010 + EncodeName(param) + EncodeTerm(body)
Apply:    0011 + EncodeTerm(function) + EncodeTerm(argument)
Constant: 0100 + EncodeType(type_tags) + EncodeConstant(value)
Force:    0101 + EncodeTerm(inner)
Error:    0110
Builtin:  0111 + EncodeBuiltin(7-bit function ID)
Constr:   1000 + EncodeNat(tag) + EncodeList(fields)    [v1.1.0+]
Case:     1001 + EncodeTerm(scrutinee) + EncodeList(branches) [v1.1.0+]
```

#### Variable/Name Encoding (De Bruijn)

De Bruijn indices are encoded as natural numbers using the variable-length encoding described below.

#### Natural Number Encoding (7-bit chunks)

Natural numbers are split into 7-bit blocks, emitted least-significant first, as a flat list:
- Each 7-bit block is preceded by a `1` bit (more data follows) or `0` bit (last block)
- Example: the number 0 encodes as `0 0000000` (stop bit + 7 zero bits)
- Example: the number 128 encodes as `1 0000000 0 0000001`

#### Integer Encoding (Zigzag + 7-bit)

Signed integers use **zigzag encoding** to convert to natural numbers first:
- `0 -> 0, -1 -> 1, 1 -> 2, -2 -> 3, 2 -> 4, ...`
- Formula: `n >= 0 ? 2*n : 2*(1-n)+1`

Then encoded as a natural number.

#### Bytestring Encoding

1. Pad the output to byte alignment first (using the padding scheme)
2. Emit chunks of up to 255 bytes, each preceded by a 1-byte length
3. Terminate with a zero-length chunk marker (`0x00`)

#### String Encoding

Strings are UTF-8 encoded to bytes, then encoded as bytestrings.

#### Boolean Encoding

A single bit: `0` for False, `1` for True.

#### List Encoding

Each element is preceded by a `1` bit; the list is terminated by a `0` bit:
```
[a, b, c] -> 1 <encode(a)> 1 <encode(b)> 1 <encode(c)> 0
[]         -> 0
```

#### Type Tag Encoding

Types are encoded as a list of 4-bit tags (using the list encoding above):
- `integer`    -> `[0]`
- `bytestring` -> `[1]`
- `string`     -> `[2]`
- `unit`       -> `[3]`
- `bool`       -> `[4]`
- `list(T)`    -> `[7, 5] ++ encode_type(T)`
- `pair(A,B)`  -> `[7, 7, 6] ++ encode_type(A) ++ encode_type(B)`
- `data`       -> `[8]`

#### Builtin Function Encoding

Builtins are encoded as a **7-bit** fixed-width natural number (their enum index 0-86).

#### Constant Value Encoding

After the type tags, the constant value is encoded according to its type:
- `integer`: zigzag + 7-bit natural number encoding
- `bytestring`: padded chunked bytestring
- `string`: UTF-8 -> bytestring encoding
- `unit`: no data emitted (only one possible value)
- `bool`: single bit
- `list(T)`: flat list encoding of elements
- `pair(A,B)`: encode first element, then second
- `data`: CBOR-encode the PlutusData, then encode the resulting bytes as a bytestring

### Layer 2: CBOR Wrapping

The flat-encoded bytes are then wrapped in CBOR. On-chain, a Plutus script appears as:

```
CBOR bytestring (CBOR bytestring (flat-encoded-UPLC-program))
```

That is, a **double CBOR-wrapped** bytestring. The outer CBOR wrapping is part of the transaction format; the inner CBOR wrapping contains the actual flat-encoded program.

To decode an on-chain script:
1. CBOR-decode the outer bytestring
2. CBOR-decode the inner bytestring to get the flat bytes
3. Flat-decode the bytes to get the UPLC `Program`

---

## 3. Plutus V2 Builtins

### Complete List (86 functions total in latest, ~54 for V2)

All builtins are referenced by their enum index (0-based), which is also their 7-bit flat encoding.

#### Integer Arithmetic (indices 0-9)

| Index | Name | Signature | Forces | Notes |
|-------|------|-----------|--------|-------|
| 0 | `AddInteger` | `integer -> integer -> integer` | 0 | |
| 1 | `SubtractInteger` | `integer -> integer -> integer` | 0 | |
| 2 | `MultiplyInteger` | `integer -> integer -> integer` | 0 | |
| 3 | `DivideInteger` | `integer -> integer -> integer` | 0 | Fails if divisor=0. Rounds toward -inf |
| 4 | `QuotientInteger` | `integer -> integer -> integer` | 0 | Fails if divisor=0. Rounds toward 0 |
| 5 | `RemainderInteger` | `integer -> integer -> integer` | 0 | Fails if divisor=0 |
| 6 | `ModInteger` | `integer -> integer -> integer` | 0 | Fails if divisor=0 |
| 7 | `EqualsInteger` | `integer -> integer -> bool` | 0 | |
| 8 | `LessThanInteger` | `integer -> integer -> bool` | 0 | |
| 9 | `LessThanEqualsInteger` | `integer -> integer -> bool` | 0 | |

#### ByteString Operations (indices 10-17)

| Index | Name | Signature | Forces |
|-------|------|-----------|--------|
| 10 | `AppendByteString` | `bytestring -> bytestring -> bytestring` | 0 |
| 11 | `ConsByteString` | `integer -> bytestring -> bytestring` | 0 |
| 12 | `SliceByteString` | `integer -> integer -> bytestring -> bytestring` | 0 |
| 13 | `LengthOfByteString` | `bytestring -> integer` | 0 |
| 14 | `IndexByteString` | `bytestring -> integer -> integer` | 0 |
| 15 | `EqualsByteString` | `bytestring -> bytestring -> bool` | 0 |
| 16 | `LessThanByteString` | `bytestring -> bytestring -> bool` | 0 |
| 17 | `LessThanEqualsByteString` | `bytestring -> bytestring -> bool` | 0 |

#### Cryptographic Hashing (indices 18-21)

| Index | Name | Signature | Forces |
|-------|------|-----------|--------|
| 18 | `Sha2_256` | `bytestring -> bytestring` | 0 |
| 19 | `Sha3_256` | `bytestring -> bytestring` | 0 |
| 20 | `Blake2b_256` | `bytestring -> bytestring` | 0 |
| 21 | `VerifyEd25519Signature` | `bytestring -> bytestring -> bytestring -> bool` | 0 |

#### String Operations (indices 22-25)

| Index | Name | Signature | Forces |
|-------|------|-----------|--------|
| 22 | `AppendString` | `string -> string -> string` | 0 |
| 23 | `EqualsString` | `string -> string -> bool` | 0 |
| 24 | `EncodeUtf8` | `string -> bytestring` | 0 |
| 25 | `DecodeUtf8` | `bytestring -> string` | 0 |

#### Control Flow / Polymorphic (indices 26-28)

| Index | Name | Signature | Forces |
|-------|------|-----------|--------|
| 26 | `IfThenElse` | `bool -> a -> a -> a` | 1 |
| 27 | `ChooseUnit` | `unit -> a -> a` | 1 |
| 28 | `Trace` | `string -> a -> a` | 1 |

#### Pair Operations (indices 29-30)

| Index | Name | Signature | Forces |
|-------|------|-----------|--------|
| 29 | `FstPair` | `pair(a,b) -> a` | 2 |
| 30 | `SndPair` | `pair(a,b) -> b` | 2 |

#### List Operations (indices 31-35)

| Index | Name | Signature | Forces |
|-------|------|-----------|--------|
| 31 | `ChooseList` | `list(a) -> b -> b -> b` | 2 |
| 32 | `MkCons` | `a -> list(a) -> list(a)` | 1 |
| 33 | `HeadList` | `list(a) -> a` | 1 |
| 34 | `TailList` | `list(a) -> list(a)` | 1 |
| 35 | `NullList` | `list(a) -> bool` | 1 |

#### Data Operations (indices 36-50)

| Index | Name | Signature | Forces |
|-------|------|-----------|--------|
| 36 | `ChooseData` | `data -> a -> a -> a -> a -> a -> a` | 1 |
| 37 | `ConstrData` | `integer -> list(data) -> data` | 0 |
| 38 | `MapData` | `list(pair(data,data)) -> data` | 0 |
| 39 | `ListData` | `list(data) -> data` | 0 |
| 40 | `IData` | `integer -> data` | 0 |
| 41 | `BData` | `bytestring -> data` | 0 |
| 42 | `UnConstrData` | `data -> pair(integer, list(data))` | 0 |
| 43 | `UnMapData` | `data -> list(pair(data,data))` | 0 |
| 44 | `UnListData` | `data -> list(data)` | 0 |
| 45 | `UnIData` | `data -> integer` | 0 |
| 46 | `UnBData` | `data -> bytestring` | 0 |
| 47 | `EqualsData` | `data -> data -> bool` | 0 |
| 48 | `MkPairData` | `data -> data -> pair(data,data)` | 0 |
| 49 | `MkNilData` | `unit -> list(data)` | 0 |
| 50 | `MkNilPairData` | `unit -> list(pair(data,data))` | 0 |

#### V2-Added Builtins (indices 51-53)

| Index | Name | Signature | Forces | Added In |
|-------|------|-----------|--------|----------|
| 51 | `SerialiseData` | `data -> bytestring` | 0 | V2 |
| 52 | `VerifyEcdsaSecp256k1Signature` | `bytestring -> bytestring -> bytestring -> bool` | 0 | V2 |
| 53 | `VerifySchnorrSecp256k1Signature` | `bytestring -> bytestring -> bytestring -> bool` | 0 | V2 |

#### V3-Added Builtins (indices 54-86)

| Index | Name | Signature | Forces | Category |
|-------|------|-----------|--------|----------|
| 54 | `Bls12_381_G1_Add` | `g1 -> g1 -> g1` | 0 | BLS |
| 55 | `Bls12_381_G1_Neg` | `g1 -> g1` | 0 | BLS |
| 56 | `Bls12_381_G1_ScalarMul` | `integer -> g1 -> g1` | 0 | BLS |
| 57 | `Bls12_381_G1_Equal` | `g1 -> g1 -> bool` | 0 | BLS |
| 58 | `Bls12_381_G1_Compress` | `g1 -> bytestring` | 0 | BLS |
| 59 | `Bls12_381_G1_Uncompress` | `bytestring -> g1` | 0 | BLS |
| 60 | `Bls12_381_G1_HashToGroup` | `bytestring -> bytestring -> g1` | 0 | BLS |
| 61 | `Bls12_381_G2_Add` | `g2 -> g2 -> g2` | 0 | BLS |
| 62 | `Bls12_381_G2_Neg` | `g2 -> g2` | 0 | BLS |
| 63 | `Bls12_381_G2_ScalarMul` | `integer -> g2 -> g2` | 0 | BLS |
| 64 | `Bls12_381_G2_Equal` | `g2 -> g2 -> bool` | 0 | BLS |
| 65 | `Bls12_381_G2_Compress` | `g2 -> bytestring` | 0 | BLS |
| 66 | `Bls12_381_G2_Uncompress` | `bytestring -> g2` | 0 | BLS |
| 67 | `Bls12_381_G2_HashToGroup` | `bytestring -> bytestring -> g2` | 0 | BLS |
| 68 | `Bls12_381_MillerLoop` | `g1 -> g2 -> mlresult` | 0 | BLS |
| 69 | `Bls12_381_MulMlResult` | `mlresult -> mlresult -> mlresult` | 0 | BLS |
| 70 | `Bls12_381_FinalVerify` | `mlresult -> mlresult -> bool` | 0 | BLS |
| 71 | `Keccak_256` | `bytestring -> bytestring` | 0 | Crypto |
| 72 | `Blake2b_224` | `bytestring -> bytestring` | 0 | Crypto |
| 73 | `IntegerToByteString` | `bool -> integer -> integer -> bytestring` | 0 | Conversion |
| 74 | `ByteStringToInteger` | `bool -> bytestring -> integer` | 0 | Conversion |
| 75 | `AndByteString` | `bool -> bytestring -> bytestring -> bytestring` | 0 | Bitwise |
| 76 | `OrByteString` | `bool -> bytestring -> bytestring -> bytestring` | 0 | Bitwise |
| 77 | `XorByteString` | `bool -> bytestring -> bytestring -> bytestring` | 0 | Bitwise |
| 78 | `ComplementByteString` | `bytestring -> bytestring` | 0 | Bitwise |
| 79 | `ReadBit` | `bytestring -> integer -> bool` | 0 | Bitwise |
| 80 | `WriteBits` | `bytestring -> list(integer) -> list(bool) -> bytestring` | 0 | Bitwise |
| 81 | `ReplicateByte` | `integer -> integer -> bytestring` | 0 | Bitwise |
| 82 | `ShiftByteString` | `bytestring -> integer -> bytestring` | 0 | Bitwise |
| 83 | `RotateByteString` | `bytestring -> integer -> bytestring` | 0 | Bitwise |
| 84 | `CountSetBits` | `bytestring -> integer` | 0 | Bitwise |
| 85 | `FindFirstSetBit` | `bytestring -> integer` | 0 | Bitwise |
| 86 | `Ripemd_160` | `bytestring -> bytestring` | 0 | Crypto |

### Builtin Availability by Plutus Version

- **Plutus V1** (Alonzo): indices 0-50 (51 builtins)
- **Plutus V2** (Babbage/Vasil): indices 0-53 (54 builtins, adds SerialiseData + SECP256k1 signatures)
- **Plutus V3** (Conway/Chang): indices 0-86 (87 builtins, adds BLS12-381, bitwise, Keccak, Blake2b-224, Ripemd-160, integer/bytestring conversions)

---

## 4. Plutus V2 vs V3 Differences

### Plutus Core Language Version

| Plutus Version | Core Version | Key Change |
|---------------|--------------|------------|
| V1, V2 | 1.0.0 | Original 8 term types |
| V3 | 1.1.0 | Adds Constr + Case (CIP-85 Sums of Products) |

### Sums of Products (CIP-85)

The biggest structural change. Before V3, algebraic data types were encoded using **Scott encoding** -- representing constructors as higher-order lambda functions:

**Scott encoding of `Just 1`:**
```
(delay (lam case_Nothing (lam case_Just [case_Just (con integer 1)])))
```

**V3 native representation of `Just 1`:**
```
(constr 0 (con integer 1))
```

**Evaluation rules for Constr/Case:**
- `Constr(tag, [f1, f2, ...])`: Creates a constructor value with a tag and fields
- `Case(scrutinee, [branch0, branch1, ...])`: Pattern matches on the scrutinee
  - If `scrutinee` evaluates to `Constr(tag, fields)` and `0 <= tag < len(branches)`:
    - Selects `branches[tag]` and applies it to each field as arguments
  - If tag is out of range: **Error**

Performance improvement: **0-30% speedup** in benchmarks vs. Scott encoding.

### Script Arguments

| Version | Spending Script Args | Other Script Args | Return |
|---------|---------------------|-------------------|--------|
| V1 | `datum, redeemer, scriptContext` (3 args) | `redeemer, scriptContext` (2 args) | Any non-error |
| V2 | `datum, redeemer, scriptContext` (3 args) | `redeemer, scriptContext` (2 args) | Any non-error |
| V3 | `scriptContext` only (1 arg) | `scriptContext` only (1 arg) | Must be `BuiltinUnit` |

In V3, the datum and redeemer are embedded within the script context itself.

### Script Purposes

| Version | Purposes |
|---------|----------|
| V1 | Spending, Minting, Certifying, Rewarding |
| V2 | Spending, Minting, Certifying, Rewarding |
| V3 | Spending, Minting, Certifying, Rewarding, **Voting, Proposing** |

### New V3 Builtins Summary

- **BLS12-381 cryptography**: 17 functions for zero-knowledge proof support
- **Bitwise operations**: 11 functions for low-level byte manipulation
- **Additional hashing**: Keccak-256, Blake2b-224, RIPEMD-160
- **Integer/ByteString conversion**: IntegerToByteString, ByteStringToInteger

### New V3 Constant Types

- `bls12_381_G1_element`
- `bls12_381_G2_element`
- `bls12_381_mlresult`

---

## 5. Existing Rust Crates

### `uplc` crate (aiken-lang)

**Crate**: [uplc on crates.io](https://crates.io/crates/uplc) (v1.1.21)
**Source**: [github.com/aiken-lang/aiken/crates/uplc](https://github.com/aiken-lang/aiken)
**License**: Apache-2.0

This is the **primary crate for our decompiler**. It provides:

#### Key Types (from `uplc::ast`)

```rust
// The program wrapper
pub struct Program<T> {
    pub version: (usize, usize, usize),
    pub term: Term<T>,
}

// The core AST -- parameterized over variable representation
pub enum Term<T> {
    Var(Rc<T>),
    Delay(Rc<Term<T>>),
    Lambda {
        parameter_name: Rc<T>,
        body: Rc<Term<T>>,
    },
    Apply {
        function: Rc<Term<T>>,
        argument: Rc<Term<T>>,
    },
    Constant(Rc<Constant>),
    Force(Rc<Term<T>>),
    Error,
    Builtin(DefaultFunction),
    Constr {
        tag: usize,
        fields: Vec<Term<T>>,
    },
    Case {
        constr: Rc<Term<T>>,
        branches: Vec<Term<T>>,
    },
}

// Variable representations
pub struct DeBruijn(usize);
pub struct NamedDeBruijn {
    pub text: String,
    pub index: DeBruijn,
}
pub struct Name {
    pub text: String,
    pub unique: Unique,
}

// Constants
pub enum Constant {
    Integer(BigInt),
    ByteString(Vec<u8>),
    String(String),
    Unit,
    Bool(bool),
    ProtoList(Type, Vec<Constant>),
    ProtoPair(Type, Type, Rc<Constant>, Rc<Constant>),
    Data(PlutusData),
    Bls12_381G1Element(Box<blst::blst_p1>),
    Bls12_381G2Element(Box<blst::blst_p2>),
    Bls12_381MlResult(Box<blst::blst_fp12>),
}

// Type descriptors (for constants)
pub enum Type {
    Bool, Integer, String, ByteString, Unit,
    List(Rc<Type>),
    Pair(Rc<Type>, Rc<Type>),
    Data,
    Bls12_381G1Element, Bls12_381G2Element, Bls12_381MlResult,
}

// Script version wrapper
pub enum SerializableProgram {
    PlutusV1Program(Program<DeBruijn>),
    PlutusV2Program(Program<DeBruijn>),
    PlutusV3Program(Program<DeBruijn>),
}
```

#### Modules

| Module | Purpose |
|--------|---------|
| `ast` | AST type definitions |
| `builtins` | `DefaultFunction` enum (all 87 builtins) |
| `flat` | Flat binary encoding/decoding |
| `machine` | CEK machine evaluator |
| `parser` | Text format parser |
| `builder` | AST construction helpers |
| `optimize` | Optimization passes |
| `tx` | Transaction-related utilities |

#### Usage for Decompilation

```rust
use uplc::ast::{Program, DeBruijn};
use uplc::flat::Flat;

// Decode from flat bytes
let program: Program<DeBruijn> = Program::from_flat(&flat_bytes)?;

// Or decode from hex string (double-CBOR-wrapped)
let program = Program::<DeBruijn>::from_hex(hex_string)?;
```

### `pallas` crate (txpipe)

**Crate**: [pallas on crates.io](https://crates.io/crates/pallas)
**Source**: [github.com/txpipe/pallas](https://github.com/txpipe/pallas)
**License**: Apache-2.0

A comprehensive Rust toolkit for Cardano. Relevant sub-crates:

| Sub-crate | Purpose |
|-----------|---------|
| `pallas-primitives` | Ledger data types (PlutusData, transactions, etc.) with CBOR codecs |
| `pallas-codec` | CBOR encoding/decoding foundation |
| `pallas-crypto` | Cryptographic operations and hash types |

#### Key Types from `pallas-primitives`

```rust
// PlutusData -- used for datum, redeemer, and script context
pub enum PlutusData {
    Constr(Constr<PlutusData>),
    Map(KeyValuePairs<PlutusData, PlutusData>),
    BigInt(BigInt),
    BoundedBytes(BoundedBytes),  // bytestrings (max 64-byte chunks)
    Array(Vec<PlutusData>),      // lists
}

pub struct Constr<A> {
    pub tag: u64,       // Constructor alternative
    pub any_constructor: Option<u64>,
    pub fields: Vec<A>,
}

pub enum BigInt {
    Int(Int),            // Small integer (fits in i64)
    BigUInt(BoundedBytes),  // Large positive (CBOR tag 2)
    BigNInt(BoundedBytes),  // Large negative (CBOR tag 3)
}
```

The `uplc` crate re-exports `PlutusData`, `Constr`, `BigInt`, `Hash`, `KeyValuePairs` from `pallas-primitives`.

### Other Relevant Crates

| Crate | Purpose |
|-------|---------|
| `pallas-uplc` | Fork of uplc-turbo tailored for use within pallas |
| `uplc-turbo` | Faster UPLC evaluator (experimental) |
| `aiken-lang` | The Aiken compiler itself (compiles Aiken -> UPLC) |

---

## 6. The Data Type

The `Data` type (called `PlutusData` in Rust) is the universal interchange format for passing arguments to Plutus scripts. Datums, redeemers, and script contexts are all encoded as `Data`.

### Definition

```haskell
data Data =
    Constr Integer [Data]       -- Tagged constructor with fields
  | Map [(Data, Data)]          -- Association list
  | List [Data]                 -- List
  | I Integer                   -- Integer
  | B ByteString                -- ByteString
```

### Semantics

- **Constr(tag, fields)**: Represents an algebraic data type constructor. `tag` identifies which alternative (0-based), `fields` are the constructor's arguments. Example: `Just 42` = `Constr(0, [I(42)])` if Just is alternative 0.
- **Map(entries)**: Ordered list of key-value pairs. Used for dictionaries/maps.
- **List(items)**: Ordered list of data items. Used for sequences.
- **I(n)**: Arbitrary-precision integer.
- **B(bytes)**: Raw bytestring. Used for hashes, public keys, etc.

### CBOR Encoding of Data

This is critical because `Data` constants inside UPLC are CBOR-encoded, then the CBOR bytes are flat-encoded as a bytestring.

#### Constructor Tags (Constr)

The CBOR encoding of `Constr(tag, fields)` uses CBOR tags to encode the alternative number:

| Constructor Tag (i) | CBOR Encoding |
|---------------------|---------------|
| 0-6 | CBOR tag `121+i`, followed by indefinite-length array of fields |
| 7-127 | CBOR tag `1280+(i-7)`, followed by indefinite-length array of fields |
| 128+ | CBOR tag `102`, followed by definite-length array `[i, fields_array]` |

Encoding formula:
```
Encode_ctag(i) =
    if 0 <= i <= 6:     CBOR_head(major=6, 121 + i)
    if 7 <= i <= 127:   CBOR_head(major=6, 1280 + (i - 7))
    otherwise:           CBOR_head(major=6, 102) + CBOR_head(major=4, 2) + Encode_int(i)
```

Decoding:
```
Decode_ctag:
    if CBOR tag 121-127:  alternative = tag - 121
    if CBOR tag 1280-1400: alternative = (tag - 1280) + 7
    if CBOR tag 102:       read [uint, fields] -> alternative = uint
```

#### Map

Encoded as CBOR major type 5 (map) with definite length, containing key-value pairs.

#### List

Encoded as CBOR major type 4 (array) with indefinite length, terminated by break byte (0xFF).

#### Integer

| Range | Encoding |
|-------|----------|
| 0 to 2^64-1 | CBOR major type 0 (unsigned int) |
| -2^64 to -1 | CBOR major type 1 (negative int) |
| >= 2^64 | CBOR tag 2 + bounded bytestring (big-endian) |
| < -2^64 | CBOR tag 3 + bounded bytestring (big-endian, encoding |n|-1) |

#### ByteString

- If <= 64 bytes: CBOR major type 2 with definite length
- If > 64 bytes: CBOR indefinite-length bytestring (major type 2, argument 31), chunked into 64-byte blocks, terminated by break byte

### Common Data Patterns in Validators

Understanding how Aiken types map to Data is essential for decompilation:

```
// Aiken Bool
True  = Constr(1, [])     // CBOR tag 122, empty array
False = Constr(0, [])     // CBOR tag 121, empty array

// Aiken Option<a>
None    = Constr(1, [])
Some(x) = Constr(0, [x])

// Aiken tuples
(a, b)    = Constr(0, [a, b])
(a, b, c) = Constr(0, [a, b, c])

// Aiken custom types (order follows declaration order)
type Action {
  Buy       // Constr(0, [])
  Sell      // Constr(1, [])
  Hold(Int) // Constr(2, [I(n)])
}
```

### Data Operations in UPLC

The builtins for working with Data form a critical pattern for decompilation:

**Constructing Data:**
- `ConstrData(tag, fields)` -> `Constr(tag, fields)`
- `MapData(pairs)` -> `Map(pairs)`
- `ListData(items)` -> `List(items)`
- `IData(n)` -> `I(n)`
- `BData(bs)` -> `B(bs)`

**Destructuring Data (these fail if the Data is the wrong variant):**
- `UnConstrData(d)` -> `(tag, fields)` as a pair
- `UnMapData(d)` -> list of pairs
- `UnListData(d)` -> list
- `UnIData(d)` -> integer
- `UnBData(d)` -> bytestring

**Inspecting Data:**
- `ChooseData(d, constr_case, map_case, list_case, int_case, bs_case)` -> selects branch based on Data variant
- `EqualsData(d1, d2)` -> structural equality

---

## Key Implications for the Decompiler

1. **Parsing**: Use the `uplc` crate's `Program::from_flat()` or `Program::from_hex()` to parse on-chain bytecode into an AST. This handles both CBOR unwrapping and flat decoding.

2. **Version Detection**: Check `program.version` to determine V1/V2 (1.0.0) vs V3 (1.1.0). Also need to know the Plutus version (V1/V2/V3) from the transaction context to know which builtins are valid.

3. **Pattern Recognition**: The decompiler needs to recognize:
   - Scott-encoded constructors (V1/V2) vs native Constr/Case (V3)
   - Force/Delay patterns around polymorphic builtins
   - Data construction/destruction patterns
   - Validator argument patterns (3 args for V1/V2 spending, 1 arg for V3)

4. **Data Type Reconstruction**: Recognize `UnConstrData` followed by tag checks to reconstruct algebraic data types. The constructor tags map directly to variant indices.

5. **Builtin Arity**: Each builtin has a known number of term arguments and force applications. The decompiler can use this to group Apply nodes around builtins.
