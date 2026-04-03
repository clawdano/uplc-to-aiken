# Aiken to UPLC Compilation Patterns Reference

A comprehensive reference for reversing Aiken-compiled UPLC back to Aiken source.
Based on Aiken v1.1.21, targeting UPLC 1.1.0 (Plutus V3) with SOP encoding.

---

## Table of Contents

1. [Compilation Pipeline Overview](#1-compilation-pipeline-overview)
2. [Type Encoding (Data/Constr)](#2-type-encoding)
3. [Validator Wrapping & Structure](#3-validator-wrapping--structure)
4. [Language Construct Patterns](#4-language-construct-patterns)
5. [Standard Library Patterns](#5-standard-library-patterns)
6. [UPLC Builtins Reference](#6-uplc-builtins-reference)
7. [Version Differences](#7-version-differences)
8. [Decompiler Heuristics](#8-decompiler-heuristics)

---

## 1. Compilation Pipeline Overview

```
Source (.ak files)
    |
    v
Parse (Module AST)
    |
    v
Type Check (CheckedModule)
    |
    v
CodeGenerator::build() -> AirTree  (Aiken Intermediate Representation)
    |
    v
AirTree::to_vec() -> Vec<Air>     (Flattened IR)
    |
    v
CodeGenerator::uplc_code_gen() -> Term<Name>  (UPLC terms)
    |
    v
Optimization passes (aiken_optimize_and_intern)
    |
    v
Program<NamedDeBruijn>  (De Bruijn indexed)
    |
    v
Flat encoding -> CBOR -> Hex (on-chain format in plutus.json)
```

### Key source files in aiken-lang compiler:
- `crates/aiken-lang/src/gen_uplc.rs` - Main code generator (~8500+ lines)
- `crates/aiken-lang/src/gen_uplc/air.rs` - AIR (intermediate representation) definitions
- `crates/aiken-lang/src/gen_uplc/tree.rs` - AirTree structure
- `crates/aiken-lang/src/gen_uplc/builder.rs` - Helper functions for UPLC construction
- `crates/aiken-lang/src/gen_uplc/decision_tree.rs` - Pattern match compilation

---

## 2. Type Encoding

### 2.1 PlutusData (the universal on-chain type)

All Aiken custom types are encoded as `PlutusData`, which has 5 forms:
- `Constr(tag: Int, fields: List<Data>)` - Tagged constructor with fields
- `Map(entries: List<Pair<Data, Data>>)` - Key-value mapping
- `List(items: List<Data>)` - Homogeneous list
- `I(value: Int)` - Integer
- `B(value: ByteString)` - Byte array

### 2.2 Constructor Tag Assignment

**Default rule**: Tags are assigned by **declaration order**, starting from 0.

```aiken
type Action {
  Mint              // tag = 0
  Burn { amount: Int }     // tag = 1
  Transfer { to: ByteArray, amount: Int }  // tag = 2
}
```

UPLC encoding of `Burn { amount: 42 }`:
```
constrData(1, [iData(42)])
```

**Explicit tags** (v1.1.19+): The `@tag(n)` annotation overrides default ordering:
```aiken
type Bool {
  @tag(0) False     // tag = 0 (regardless of position)
  @tag(1) True      // tag = 1
}
```

### 2.3 Built-in Type Encodings

| Aiken Type | Data Encoding | Notes |
|-----------|--------------|-------|
| `Bool` | `Constr(0, [])` = False, `Constr(1, [])` = True | Tag 0=False, 1=True |
| `Void` / `()` | `Constr(0, [])` | Unit type |
| `Option<a>` | `Constr(1, [a])` = Some, `Constr(0, [])` = None | Note: Some=1, None=0 |
| `Int` | `I(n)` | Arbitrary precision |
| `ByteArray` | `B(bytes)` | Raw bytes |
| `String` | `B(encode_utf8(s))` | Encoded as ByteArray via encode_utf8 |
| `List<a>` | `List([...])` | Native UPLC list when non-Data; `listData` when Data |
| `Pair<a,b>` | Encoded as 2-element list `List([a, b])` | Converted via mk_pair_data |
| `Dict<k,v>` (via Pairs) | `Map([(k,v), ...])` | Map of key-value Data pairs |

### 2.4 Record/Struct Encoding

Single-constructor types use tag 0 by default:
```aiken
type Datum {
  owner: ByteArray,    // field 0
  count: Int,          // field 1
}
// Encodes as: Constr(0, [B(owner_bytes), I(count)])
```

Fields are ordered by **declaration order** and stored in the Constr's field list.

### 2.5 The `@list` Decorator

Types annotated with `@list` use `PlutusList` instead of `Constr`:
```aiken
@list
type MyPair {
  fst: Int,
  snd: Int,
}
// Encodes as: List([I(fst), I(snd)]) instead of Constr(0, [I(fst), I(snd)])
```

### 2.6 Type Conversion Functions in UPLC

**Wrapping values into Data** (`convert_type_to_data`):
| Source Type | UPLC Pattern |
|------------|-------------|
| Int | `iData(value)` |
| ByteArray | `bData(value)` |
| String | `bData(encodeUtf8(value))` |
| List | `listData(value)` |
| Map | `mapData(value)` |
| Pair | `listData(mkCons(fst, mkCons(snd, [])))` |
| BLS G1 | `bData(bls12_381_g1_compress(value))` |
| Constr/custom | Already Data, no wrapping needed |

**Unwrapping Data to values** (`unknown_data_to_type`):
| Target Type | UPLC Pattern |
|------------|-------------|
| Int | `unIData(value)` |
| ByteArray | `unBData(value)` |
| String | `decodeUtf8(unBData(value))` |
| List | `unListData(value)` |
| Map | `unMapData(value)` |
| Pair | `mkPairData(headList(unListData(x)), headList(tailList(unListData(x))))` |
| BLS G1 | `bls12_381_g1_uncompress(unBData(value))` |

---

## 3. Validator Wrapping & Structure

### 3.1 Overall Validator Shape

A compiled Aiken validator has this structure in UPLC:

```
program 1.1.0
(lam __context__
  <special_functions_wrapper>
    <validator_body>
)
```

The outermost lambda takes a **single argument**: the script context (as raw Data).

### 3.2 Special Functions Preamble

Aiken hoists commonly-used builtin operations into a SOP tuple at the top of the program. In UPLC 1.1.0 (Plutus V3), this appears as:

```
(lam i_0           ; __context__
  (case
    (constr 0       ; Pack builtins into a SOP tuple
      (force (builtin chooseData))
      (force (builtin tailList))
      (force (builtin headList))
      (force (force (builtin chooseList)))
      (force (force (builtin sndPair)))
      (force (force (builtin fstPair)))
      (force (builtin trace))
      (force (builtin ifThenElse)))
    (lam i_1        ; chooseData
      (lam i_2      ; tailList
        (lam i_3    ; headList
          ...       ; etc - one lam per builtin
            <actual validator logic>
```

**Key decompiler pattern**: The opening `(case (constr 0 ...builtins...) (lam ... (lam ...)))` is Aiken's builtin preamble. The number and order of builtins varies by what the validator actually uses. Common builtins packed this way:

- `tailList` (forced once)
- `headList` (forced once)
- `chooseList` (forced twice)
- `sndPair` (forced twice)
- `fstPair` (forced twice)
- `ifThenElse` (forced once)
- `trace` (forced once)
- `chooseData` (forced once)

### 3.3 CONSTR_FIELDS_EXPOSER and CONSTR_INDEX_EXPOSER

Two special helper functions are always available (defined in `CodeGenSpecialFuncs`):

```
CONSTR_FIELDS_EXPOSER = lam __constr_var -> sndPair(unConstrData(__constr_var))
CONSTR_INDEX_EXPOSER  = lam __constr_var -> fstPair(unConstrData(__constr_var))
```

These destructure a `Constr` Data value into its tag (index) and fields list.

### 3.4 Validator Condition Wrapping (`wrap_validator_condition`)

Validators must return `Bool`, but UPLC validators signal success/failure via unit/error. Aiken wraps the validator body:

```
if validator_result_bool then
  ()                           -- success: return unit
else
  error                        -- failure: abort
```

In UPLC (with traces enabled - verbose mode):
```
(force
  (case
    (constr 0 <validator_bool_result>)
    (delay (con unit ()))                                    ; True branch -> ()
    (delay
      (force [
        [trace "Validator returned false"]
        (delay [error (force error)])
      ]))                                                    ; False branch -> trace + error
  )
  ifThenElse)
```

In silent mode, the False branch is simply `(error)` without the trace.

**Decompiler pattern**: Look for the terminal `(con unit ())` vs `(error)` branching at the outermost level. This is the validator wrapper.

### 3.5 Script Context Destructuring

The single `__context__` argument is destructured into:
- `__transaction__` - The transaction
- `__redeemer__` - The redeemer (as raw Data)
- `__purpose__` - The script purpose (spend, mint, etc.)

This appears as an `expect` pattern on `ScriptContext`:
```
-- ScriptContext = Constr(0, [transaction, redeemer, purpose])
let fields = sndPair(unConstrData(context))
let transaction = headList(fields)
let redeemer = headList(tailList(fields))
let purpose = headList(tailList(tailList(fields)))
```

### 3.6 Multi-handler Dispatch (spend, mint, etc.)

After destructuring the script context, Aiken generates a `when` on the purpose to dispatch to the correct handler. The purpose is a tagged constructor:

| Purpose | Tag |
|---------|-----|
| Mint | 0 |
| Spend | 1 |
| Withdraw | 2 |
| Publish | 3 |
| Vote | 4 |
| Propose | 5 |

For `spend`, the datum is extracted from the purpose args (it's `Option<Datum>`).

### 3.7 Validator Parameter Casting (`cast_validator_args`)

If a validator has parameters, they are wrapped in lambdas and cast from Data:
```
lam param_name -> (
  lam param_name -> (   ; shadows with cast version
    <validator_body>
  )(known_data_to_type(param_name))
)
```

For each parameter, there's a lambda + immediate application with a type cast.

---

## 4. Language Construct Patterns

### 4.1 `let` Bindings

Aiken `let` compiles to lambda application (the standard functional encoding):

```aiken
let x = 42
x + 1
```

UPLC:
```
[(lam x [addInteger x (con integer 1)]) (con integer 42)]
```

Pattern: `(lam <name> <body>) applied_to <value>`

### 4.2 `if/else` Expressions

```aiken
if condition { then_branch } else { else_branch }
```

UPLC (uses `delayed_if_then_else`):
```
(force [
  [
    [ifThenElse condition]
    (delay then_branch)
  ]
  (delay else_branch)
])
```

The branches are `delay`ed to prevent eager evaluation, then `force`d after selection.

**Key pattern**: `force(ifThenElse(cond, delay(a), delay(b)))` = `if cond then a else b`

### 4.3 Pattern Matching (`when/is`)

Pattern matching uses **decision trees** compiled via `decision_tree.rs`. The compilation strategy depends on what's being matched:

#### Matching on Bool:
```aiken
when x is { True -> a, False -> b }
```
UPLC:
```
(force [
  [ifThenElse x (delay a)]
  (delay b)
])
```

#### Matching on custom constructors:
```aiken
when action is {
  Mint -> 0
  Burn { amount } -> amount
  Transfer { to, amount } -> amount
}
```

UPLC uses constructor index comparison + field extraction:
```
let constr_index = fstPair(unConstrData(action))
let constr_fields = sndPair(unConstrData(action))
force(case(constr 0
  [equalsInteger 0 constr_index]      ; is it Mint (tag 0)?
  (delay <mint_body>)                  ; yes -> Mint branch
  (delay                               ; no ->
    force(case(constr 0
      [equalsInteger 1 constr_index]  ; is it Burn (tag 1)?
      (delay                           ; yes ->
        let amount = unIData(headList(constr_fields))
        <burn_body>
      )
      (delay                           ; no -> must be Transfer (tag 2)
        let to = unBData(headList(constr_fields))
        let amount = unIData(headList(tailList(constr_fields)))
        <transfer_body>
      )
    ) ifThenElse)
  )
) ifThenElse)
```

**Key patterns for decompiler**:
- `equalsInteger(N, fstPair(unConstrData(x)))` = checking if `x` is constructor with tag N
- `headList(sndPair(unConstrData(x)))` = accessing first field
- `headList(tailList(sndPair(unConstrData(x))))` = accessing second field
- Chain of `tailList` calls = indexing deeper into fields

#### Matching on lists:
```aiken
when xs is {
  [] -> 0
  [head, ..tail] -> head + sum(tail)
}
```
UPLC uses `chooseList`:
```
force(chooseList(xs,
  (delay 0),                    ; empty case
  (delay                        ; non-empty case
    let head = headList(xs)
    let tail = tailList(xs)
    addInteger(head, sum(tail))
  )
))
```

### 4.4 `expect` Statements

`expect` is non-exhaustive pattern matching that errors on mismatch.

#### expect on Option:
```aiken
expect Some(value) = maybe_val
```
UPLC:
```
let constr_pair = unConstrData(maybe_val)
let tag = fstPair(constr_pair)
force(case(constr 0
  [equalsInteger 1 tag]           ; Some has tag 1
  (delay
    let fields = sndPair(constr_pair)
    let value = headList(fields)
    <continuation>
  )
  (delay (error))                  ; mismatch -> crash
) ifThenElse)
```

With verbose tracing, the error branch includes:
```
(delay (force [trace "<error_message>" (delay error)]))
```

#### expect for Data casting:
```aiken
expect my_datum: MyType = some_data
```
UPLC:
```
let my_datum = unknown_data_to_type(some_data)  ; calls unConstrData etc.
<continuation using my_datum>
```

### 4.5 Function Definitions

Functions are compiled to lambda abstractions. Named functions are hoisted to their usage scope.

```aiken
fn add(a: Int, b: Int) -> Int {
  a + b
}
```

UPLC (after hoisting):
```
(lam add
  <body_that_uses_add>
)
(lam a (lam b [addInteger a b]))
```

**Recursive functions** use self-application (Y-combinator style):
```
(lam f
  [(lam s [s s]) (lam s
    (lam <params>
      <body where recursive call = [s s <args>]>
    )
  )]
)
```

**Cyclic/mutually-recursive functions** use a more complex encoding with multiple function names threaded through.

### 4.6 Function Calls

```aiken
add(1, 2)
```

UPLC:
```
[[add (con integer 1)] (con integer 2)]
```

Functions are curried: multi-argument functions become chains of single-argument applications.

### 4.7 Record/Field Access

```aiken
datum.count   // where count is field at index 1
```

UPLC: Generates an access function `__access_index_1`:
```
headList(tailList(sndPair(unConstrData(datum))))
```

The pattern is: `headList` after N `tailList` calls on the constructor's field list, where N is the field index.

For `@list`-decorated types, `unConstrData` and `sndPair` are skipped - fields come directly from `unListData`.

### 4.8 `trace` Function

```aiken
trace @"my message"
expr
```

UPLC:
```
(force [
  [trace (con string "my message")]
  (delay expr)
])
```

The `trace` builtin takes a string and returns a delayed value. The `delayed_trace` pattern wraps the continuation in delay/force.

**Trace levels** (build-time):
- `silent`: All trace calls are **erased entirely** from the output
- `compact`: Only the label (first element) is preserved
- `verbose`: Full trace with all arguments preserved

### 4.9 List Construction

```aiken
[1, 2, 3]
```

UPLC:
```
mkCons(iData(1), mkCons(iData(2), mkCons(iData(3), [])))
```

For lists of Data items, elements are wrapped with the appropriate `*Data` function. The empty list is `mkNilData(())` or `mkNilPairData(())` for pair lists.

### 4.10 Binary Operators

| Aiken | UPLC Builtin |
|-------|-------------|
| `+` | `addInteger` |
| `-` | `subtractInteger` |
| `*` | `multiplyInteger` |
| `/` | `divideInteger` |
| `%` | `modInteger` |
| `==` (Int) | `equalsInteger` |
| `==` (ByteArray) | `equalsByteString` |
| `==` (String) | `equalsString` |
| `==` (Data) | `equalsData` |
| `<` | `lessThanInteger` |
| `<=` | `lessThanEqualsInteger` |
| `>` | args swapped + `lessThanInteger` |
| `>=` | args swapped + `lessThanEqualsInteger` |
| `&&` | `ifThenElse(a, b, False)` |
| `\|\|` | `ifThenElse(a, True, b)` |
| `!` | `ifThenElse(x, False, True)` |

### 4.11 Pipe Operator

```aiken
x |> f |> g
```

Compiles to nested function application:
```
g(f(x))
```

No special UPLC pattern - just syntactic sugar.

### 4.12 Record Updates

```aiken
Person { ..person, age: person.age + 1 }
```

UPLC: Constructs a new Constr with the modified fields, accessing original fields from the old record for unchanged fields.

---

## 5. Standard Library Patterns

### 5.1 List Functions

All stdlib list functions are written in Aiken itself (no special builtin treatment). They compile to recursive UPLC functions.

**list.map**:
```aiken
pub fn map(self: List<a>, with: fn(a) -> result) -> List<result> {
  when self is {
    [] -> []
    [x, ..xs] -> [with(x), ..map(xs, with)]
  }
}
```
UPLC pattern: Recursive function that uses `chooseList` for empty check, `headList`/`tailList` for destructuring, `mkCons` for construction.

**list.filter**: Same recursive pattern with an `ifThenElse` for the predicate.

**list.foldl/foldr**: Recursive with accumulator parameter.

**list.any/all**: Recursive with short-circuit via `ifThenElse` (using `||` / `&&`).

### 5.2 Dict/Map Operations

Aiken's `Dict` is built on sorted association lists (list of pairs). Operations like `dict.get`, `dict.insert` compile to list traversals.

### 5.3 Common Patterns to Recognize

| UPLC Pattern | Likely Aiken Construct |
|-------------|----------------------|
| Recursive function with `chooseList` + `headList`/`tailList` + `mkCons` | `list.map` or similar list transformation |
| Recursive with `chooseList` + `ifThenElse` + `mkCons` | `list.filter` |
| Recursive with `chooseList` + accumulator | `list.foldl` / `list.foldr` |
| `equalsData` comparison in a list traversal | `list.find` or `dict.get` |
| `headList` + `tailList` chain on `unMapData` | Dict/Map access |

---

## 6. UPLC Builtins Reference

### Data Operations
| Builtin | Signature | Purpose |
|---------|----------|---------|
| `constrData` | (Int, List Data) -> Data | Create Constr |
| `unConstrData` | Data -> Pair<Int, List Data> | Destructure Constr |
| `iData` | Int -> Data | Wrap int |
| `unIData` | Data -> Int | Unwrap int |
| `bData` | ByteString -> Data | Wrap bytes |
| `unBData` | Data -> ByteString | Unwrap bytes |
| `listData` | List Data -> Data | Wrap list |
| `unListData` | Data -> List Data | Unwrap list |
| `mapData` | List Pair<Data,Data> -> Data | Wrap map |
| `unMapData` | Data -> List Pair<Data,Data> | Unwrap map |
| `chooseData` | (Data, a, a, a, a, a) -> a | Branch by Data constructor (constr/map/list/int/bytes) |
| `equalsData` | (Data, Data) -> Bool | Data equality |

### List Operations
| Builtin | Purpose |
|---------|---------|
| `mkCons` | Prepend element to list |
| `headList` | First element |
| `tailList` | All but first |
| `nullList` | Is empty? |
| `chooseList` | Branch on empty/nonempty |
| `mkNilData` | Empty Data list |
| `mkNilPairData` | Empty pair list |

### Pair Operations
| Builtin | Purpose |
|---------|---------|
| `fstPair` | First element of pair |
| `sndPair` | Second element of pair |
| `mkPairData` | Create pair of Data |

### Control Flow
| Builtin | Purpose |
|---------|---------|
| `ifThenElse` | Bool conditional |
| `trace` | Debug tracing |

### SOP-Specific (UPLC 1.1.0 / Plutus V3)
| Term | Purpose |
|------|---------|
| `constr i [args...]` | Create a SOP constructor value with tag i |
| `case scrut [handlers...]` | Pattern match on a SOP value, dispatch to handler by tag |

---

## 7. Version Differences

### Plutus V1/V2 (UPLC 1.0.0) vs Plutus V3 (UPLC 1.1.0)

**Encoding strategy**:
- V1/V2: **Scott encoding** - constructors are lambda functions; matching costs O(k) for k constructors
- V3: **Sums-of-Products (SOP)** - native `constr`/`case` AST nodes; constant-time matching

**Scott encoding example** (V1/V2):
```
-- Bool True = \true_handler false_handler -> true_handler
-- Bool False = \true_handler false_handler -> false_handler
-- Pattern match: apply the value to handlers
[bool_value true_branch false_branch]
```

**SOP encoding example** (V3):
```
-- Bool True = constr 1
-- Bool False = constr 0
-- Pattern match:
case bool_value [false_handler, true_handler]
```

### Key Aiken Version Changes

| Version | Change | Impact on UPLC |
|---------|--------|---------------|
| v1.0.x | Plutus V1/V2 support, Scott encoding | Constructors as lambdas |
| v1.1.0+ | Plutus V3, SOP encoding | `constr`/`case` terms instead of Scott |
| v1.1.19 | `@tag(n)` attribute | Constructor tags can differ from declaration order |
| v1.1.x | `@list` decorator | Types encoded as PlutusList instead of Constr |
| v1.1.x | Constants evaluated at compile-time | More constant folding in output |
| v1.1.x | 10-20% optimization improvements | case-constr optimization, function rearranging, inlining in if_then_else_error |
| v1.1.x | Pair type | Dedicated pair handling instead of 2-element tuples |
| v1.1.21 | Latest stable | V3-only in aiken.toml |

### Optimization Passes

The compiler applies these optimizations after UPLC generation:
1. Remove identity function applications: `(lam x x) v` -> `v`
2. Beta reduction for constants: `(lam x body) constant` -> `body[x := constant]`
3. Inline single-use variables
4. Inline zero-use variables (dead code elimination)
5. Constant folding: evaluate constant expressions at compile time

---

## 8. Decompiler Heuristics

### 8.1 Recognizing the Preamble

The first thing in any Aiken-compiled validator:
1. Outer `(lam i_0 ...)` - the context parameter
2. `(case (constr 0 <forced_builtins>) ...)` - builtin preamble
3. Chain of `(lam i_N ...)` - binding each builtin to a variable
4. These variables (i_1, i_2, etc.) are used throughout instead of repeating builtin names

**To decompile**: Map each `i_N` back to its builtin by tracing the `constr 0` arguments.

### 8.2 Recognizing Validator vs. Error

Look for the outermost `if/then/else` that returns `(con unit ())` on True and `(error)` (or `trace + error`) on False. Everything inside that conditional is the validator body.

### 8.3 Recognizing Custom Types

- `constrData(N, [fields...])` = constructing a value of tag N
- `unConstrData` followed by `fstPair`/`sndPair` = destructuring a custom type
- `equalsInteger(N, fstPair(unConstrData(x)))` = checking constructor tag

### 8.4 Recognizing Field Access

- `headList(sndPair(unConstrData(x)))` = field 0
- `headList(tailList(sndPair(unConstrData(x))))` = field 1
- `headList(tailList(tailList(sndPair(unConstrData(x)))))` = field 2
- Count the `tailList` calls to determine the field index

### 8.5 Recognizing Control Flow

- `force(ifThenElse(cond, delay(a), delay(b)))` = `if cond then a else b`
- `chooseList(xs, delay(empty_case), delay(nonempty_case))` = list pattern match
- `equalsInteger(N, tag) -> if_then_else` chain = `when` on custom type
- `case(constr 0 [...] handler1 handler2)` with `ifThenElse` = SOP-based conditional

### 8.6 Recognizing let Bindings

- `(lam name body) value` = `let name = value` followed by `body`
- After optimization, simple lets may be inlined away

### 8.7 Recognizing expect

- Same as pattern match but with `(error)` in the non-matching branch
- May include trace message before error in verbose mode
- Verbose trace pattern: `force(trace(msg, delay(error)))`

### 8.8 Recognizing Recursion

- Look for self-application pattern: `(lam s [s s ...])`
- The function takes itself as first argument
- Recursive calls appear as `[s s <args>]`

### 8.9 SOP Pattern (Plutus V3)

In UPLC 1.1.0, the `case`/`constr` terms are heavily used:
- `(constr 0 arg1 arg2 ...)` packs values into a tuple
- `(case (constr 0 ...) handler)` immediately destructures
- This is used both for builtins preamble AND for actual data type operations
- **Distinguish**: Preamble `constr 0` is at the top level with forced builtins as args; data-level `constr N` uses Data values

### 8.10 Common Error Patterns

- `(error)` alone = simple error/failure (used in `fail`, `todo`, failed `expect`)
- `[(error) (force (error))]` = validator error (double-error pattern for special signaling)
- `force(trace("message", delay(error)))` = traced error (verbose mode)

---

## Appendix: Concrete UPLC Example

### Minimal spend validator (always True)

```aiken
validator minimal {
  spend(_datum: Option<Data>, _redeemer: Data, _oref: Data, _ctx: Data) {
    True
  }
}
```

Compiles to (UPLC 1.1.0, verbose trace):

```
(program 1.1.0
  (lam i_0                                    ; __context__
    (case
      (constr 0                                ; Pack builtins
        (force (builtin tailList))             ; i_1
        (force (builtin headList))             ; i_2
        (force (force (builtin sndPair)))      ; i_3
        (force (force (builtin fstPair)))      ; i_4
        (force (builtin trace))                ; i_5
        (force (builtin ifThenElse)))          ; i_6
      (lam i_1 (lam i_2 (lam i_3 (lam i_4 (lam i_5 (lam i_6
        (force
          (case
            (constr 0
              (con bool True)                  ; The validator just returns True
            )
            (delay (con unit ()))              ; True -> success (unit)
            (delay
              (force [                          ; False -> trace error
                [i_5 (con string "Validator returned false")]
                (delay [(error) (force (error))])
              ])
            )
          ) i_6)                               ; dispatch via ifThenElse
        )
      ))))))
    )
  )
)
```

**Note**: Even this "always True" validator includes:
- Builtin preamble (6 builtins packed via SOP)
- Script context destructuring (to extract datum from spend purpose)
- The wrap_validator_condition (True -> unit, False -> error)
- Verbose trace on failure path

---

## Sources

- [Aiken UPLC Documentation](https://aiken-lang.org/uplc)
- [Aiken UPLC Syntax](https://aiken-lang.org/uplc/syntax)
- [Aiken UPLC Builtins](https://aiken-lang.org/uplc/builtins)
- [Aiken Custom Types](https://aiken-lang.org/language-tour/custom-types)
- [Aiken Control Flow](https://aiken-lang.org/language-tour/control-flow)
- [Aiken Validators](https://aiken-lang.org/language-tour/validators)
- [Aiken Hello World](https://aiken-lang.org/example--hello-world/basics)
- [Aiken Troubleshooting (Tracing)](https://aiken-lang.org/language-tour/troubleshooting)
- [UPLC Code Gen Optimizations - Issue #135](https://github.com/aiken-lang/aiken/issues/135)
- [Tracing Aiken Build (kompact.io)](https://kompact.io/posts/tracing-aiken-build.html)
- [Encoding Data Types in UPLC (Plutus docs)](https://plutus.cardano.intersectmbo.org/docs/delve-deeper/encoding)
- [Plutonomicon UPLC Reference](https://plutonomicon.github.io/plutonomicon/uplc)
- [CIP-0085: Sums-of-products in Plutus Core](https://cips.cardano.org/cip/CIP-0085)
- [CIP-0057: Plutus Contract Blueprint](https://cips.cardano.org/cip/CIP-0057)
- [Aiken Evolution: Alpha to GA (PRAGMA)](https://pragma.builders/blog/2024-09-04-aiken-from-alpha-to-general-availability/)
- [Aiken compiler source - gen_uplc.rs](https://github.com/aiken-lang/aiken/blob/main/crates/aiken-lang/src/gen_uplc.rs)
- [Aiken compiler source - builder.rs](https://github.com/aiken-lang/aiken/blob/main/crates/aiken-lang/src/gen_uplc/builder.rs)
- [Aiken compiler source - air.rs](https://github.com/aiken-lang/aiken/blob/main/crates/aiken-lang/src/gen_uplc/air.rs)
- [Aiken compiler source - decision_tree.rs](https://github.com/aiken-lang/aiken/blob/main/crates/aiken-lang/src/gen_uplc/decision_tree.rs)
- [Aiken stdlib - list.ak](https://github.com/aiken-lang/stdlib/blob/main/lib/aiken/collection/list.ak)
