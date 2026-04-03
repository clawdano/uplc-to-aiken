use aiken/collection/list
use aiken/crypto.{blake2b_256, VerificationKeyHash}
use aiken/primitive/bytearray

// ============================================================
// 1. Nested custom types + deep pattern matching
// ============================================================

type TokenInfo {
  policy_id: ByteArray,
  asset_name: ByteArray,
  amount: Int,
}

type SwapDirection {
  Buy { token: TokenInfo, max_price: Int }
  Sell { token: TokenInfo, min_price: Int }
}

type OrderStatus {
  Open
  PartiallyFilled { remaining: Int }
  Filled
  Cancelled
}

type Order {
  owner: ByteArray,
  direction: SwapDirection,
  status: OrderStatus,
}

fn evaluate_order(order: Order) -> Bool {
  when order.direction is {
    Buy { token, max_price } ->
      when order.status is {
        Open -> token.amount > 0 && max_price > 0
        PartiallyFilled { remaining } ->
          remaining > 0 && remaining < token.amount
        Filled -> False
        Cancelled -> False
      }
    Sell { token, min_price } ->
      when order.status is {
        Open -> token.amount > 0 && min_price > 0
        PartiallyFilled { remaining } ->
          remaining > 0 && remaining <= token.amount && min_price > 0
        Filled -> False
        Cancelled -> False
      }
  }
}

validator nested_pattern {
  spend(_datum: Option<Data>, redeemer: Data, _oref: Data, _tx: Data) {
    expect order: Order = redeemer
    evaluate_order(order)
  }
}

// ============================================================
// 2. List operations
// ============================================================

fn sum_list(xs: List<Int>) -> Int {
  list.foldl(xs, 0, fn(x, acc) { x + acc })
}

fn double_list(xs: List<Int>) -> List<Int> {
  list.map(xs, fn(x) { x * 2 })
}

fn keep_positive(xs: List<Int>) -> List<Int> {
  list.filter(xs, fn(x) { x > 0 })
}

fn find_first_gt(xs: List<Int>, threshold: Int) -> Option<Int> {
  list.find(xs, fn(x) { x > threshold })
}

fn check_any_negative(xs: List<Int>) -> Bool {
  list.any(xs, fn(x) { x < 0 })
}

fn check_all_positive(xs: List<Int>) -> Bool {
  list.all(xs, fn(x) { x > 0 })
}

validator list_ops {
  spend(_datum: Option<Data>, redeemer: Data, _oref: Data, _tx: Data) {
    expect values: List<Int> = redeemer
    let doubled = double_list(values)
    let positives = keep_positive(doubled)
    let total = sum_list(positives)
    let has_big = find_first_gt(positives, 100)
    let no_neg = !check_any_negative(values)
    let all_pos = check_all_positive(positives)
    total > 0 && no_neg && all_pos && has_big != None
  }
}

// ============================================================
// 3. Transaction context - using typed handler params
// ============================================================

// We access transaction fields through a separate validator file
// to avoid issues with opaque Value type in Transaction

type TxInfo {
  signers: List<VerificationKeyHash>,
  input_count: Int,
  output_count: Int,
}

fn has_signer(signers: List<VerificationKeyHash>, target: VerificationKeyHash) -> Bool {
  list.any(signers, fn(s) { s == target })
}

fn count_matching(xs: List<Int>, predicate: fn(Int) -> Bool) -> Int {
  list.foldl(xs, 0, fn(x, acc) {
    if predicate(x) {
      acc + 1
    } else {
      acc
    }
  })
}

validator tx_info_check {
  spend(_datum: Option<Data>, redeemer: Data, _oref: Data, _tx: Data) {
    expect info: TxInfo = redeemer
    let signed = list.length(info.signers) > 0
    let has_inputs = info.input_count > 0
    let has_outputs = info.output_count > 0
    let values = [info.input_count, info.output_count]
    let positive_count = count_matching(values, fn(v) { v > 0 })
    signed && has_inputs && has_outputs && positive_count == 2
  }
}

// ============================================================
// 4. Recursive helper functions
// ============================================================

fn fibonacci(n: Int) -> Int {
  if n <= 0 {
    0
  } else if n == 1 {
    1
  } else {
    fibonacci(n - 1) + fibonacci(n - 2)
  }
}

fn power(base: Int, exp: Int) -> Int {
  if exp <= 0 {
    1
  } else {
    base * power(base, exp - 1)
  }
}

fn gcd(a: Int, b: Int) -> Int {
  if b == 0 {
    a
  } else {
    gcd(b, a % b)
  }
}

fn count_items(xs: List<Int>, acc: Int) -> Int {
  when xs is {
    [] -> acc
    [_, ..rest] -> count_items(rest, acc + 1)
  }
}

validator recursive_fns {
  spend(_datum: Option<Data>, redeemer: Data, _oref: Data, _tx: Data) {
    expect n: Int = redeemer
    let fib = fibonacci(n)
    let pw = power(2, n)
    let g = gcd(fib, pw)
    let count = count_items([1, 2, 3, 4, 5], 0)
    fib > 0 && pw > 0 && g > 0 && count == 5
  }
}

// ============================================================
// 5. Bytearray / hash operations
// ============================================================

fn verify_hash(data: ByteArray, expected_hash: ByteArray) -> Bool {
  let computed = blake2b_256(data)
  computed == expected_hash
}

fn check_prefix(bs: ByteArray, prefix: ByteArray) -> Bool {
  bytearray.starts_with(bs, prefix)
}

fn concat_and_hash(a: ByteArray, b: ByteArray) -> ByteArray {
  let combined = bytearray.concat(a, b)
  blake2b_256(combined)
}

type HashInput {
  data: ByteArray,
  expected: ByteArray,
  prefix: ByteArray,
}

validator hash_ops {
  spend(_datum: Option<Data>, redeemer: Data, _oref: Data, _tx: Data) {
    expect input: HashInput = redeemer
    let hash_ok = verify_hash(input.data, input.expected)
    let prefix_ok = check_prefix(input.data, input.prefix)
    let combined_hash = concat_and_hash(input.data, input.expected)
    let len_ok = bytearray.length(combined_hash) == 32
    hash_ok && prefix_ok && len_ok
  }
}

// ============================================================
// 6. Minting policy
// ============================================================

type MintAction {
  MintTokens { count: Int }
  BurnTokens { count: Int }
}

validator token_minter {
  mint(redeemer: Data, _policy_id: Data, _tx: Data) {
    expect action: MintAction = redeemer
    when action is {
      MintTokens { count } -> count > 0
      BurnTokens { count } -> count > 0
    }
  }
}
