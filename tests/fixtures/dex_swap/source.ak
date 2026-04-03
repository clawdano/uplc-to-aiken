use aiken/collection/list
use aiken/crypto.{VerificationKeyHash}

// ============================================================
// Realistic DEX Swap Validator
// ============================================================

type PoolDatum {
  token_a_policy: ByteArray,
  token_a_name: ByteArray,
  token_b_policy: ByteArray,
  token_b_name: ByteArray,
  reserve_a: Int,
  reserve_b: Int,
  total_liquidity: Int,
  fee_numerator: Int,
  fee_denominator: Int,
  admin: VerificationKeyHash,
}

type SwapRedeemer {
  Swap { min_received: Int, signers: List<VerificationKeyHash> }
  AddLiquidity {
    token_a_amount: Int,
    token_b_amount: Int,
    signers: List<VerificationKeyHash>,
  }
  RemoveLiquidity { lp_amount: Int, signers: List<VerificationKeyHash> }
  UpdateFee {
    new_numerator: Int,
    new_denominator: Int,
    signers: List<VerificationKeyHash>,
  }
}

fn is_signed_by(
  signers: List<VerificationKeyHash>,
  target: VerificationKeyHash,
) -> Bool {
  list.any(signers, fn(s) { s == target })
}

fn compute_swap_output(
  input_amount: Int,
  input_reserve: Int,
  output_reserve: Int,
  fee_num: Int,
  fee_den: Int,
) -> Int {
  // AMM constant product with fee
  // output = (input * fee_num * output_reserve) / (input_reserve * fee_den + input * fee_num)
  let numerator = input_amount * fee_num * output_reserve
  let denominator = input_reserve * fee_den + input_amount * fee_num
  numerator / denominator
}

fn validate_reserves(pool: PoolDatum) -> Bool {
  pool.reserve_a > 0 && pool.reserve_b > 0
}

fn validate_fee_params(num: Int, den: Int) -> Bool {
  num > 0 && den > 0 && num < den
}

fn compute_liquidity_tokens(
  amount_a: Int,
  amount_b: Int,
  reserve_a: Int,
  reserve_b: Int,
  total_lp: Int,
) -> Int {
  // LP tokens proportional to contribution
  let lp_from_a = amount_a * total_lp / reserve_a
  let lp_from_b = amount_b * total_lp / reserve_b
  // Take the minimum to prevent manipulation
  if lp_from_a < lp_from_b {
    lp_from_a
  } else {
    lp_from_b
  }
}

fn validate_constant_product(
  old_a: Int,
  old_b: Int,
  new_a: Int,
  new_b: Int,
) -> Bool {
  // k = a * b must not decrease
  new_a * new_b >= old_a * old_b
}

validator dex_swap {
  spend(datum: Option<Data>, redeemer: Data, _oref: Data, _tx: Data) {
    expect Some(raw_datum) = datum
    expect pool: PoolDatum = raw_datum
    expect action: SwapRedeemer = redeemer

    // Validate pool reserves are positive
    expect True = validate_reserves(pool)

    // Validate fee parameters
    expect True =
      validate_fee_params(pool.fee_numerator, pool.fee_denominator)

    when action is {
      Swap { min_received, signers } -> {
        // Must have at least one signer
        expect True = list.length(signers) > 0

        // Compute expected output for a swap of 1000 units of token A
        let swap_amount = 1000
        let expected_out =
          compute_swap_output(
            swap_amount,
            pool.reserve_a,
            pool.reserve_b,
            pool.fee_numerator,
            pool.fee_denominator,
          )

        // Output must meet minimum
        let meets_min = expected_out >= min_received

        // Validate constant product invariant
        let new_reserve_a = pool.reserve_a + swap_amount
        let new_reserve_b = pool.reserve_b - expected_out
        let k_preserved =
          validate_constant_product(
            pool.reserve_a,
            pool.reserve_b,
            new_reserve_a,
            new_reserve_b,
          )

        meets_min && k_preserved && expected_out > 0
      }
      AddLiquidity { token_a_amount, token_b_amount, signers } -> {
        // Must have signer
        expect True = list.length(signers) > 0

        // Amounts must be positive
        let amounts_valid = token_a_amount > 0 && token_b_amount > 0

        // Compute LP tokens to mint
        let lp_tokens =
          compute_liquidity_tokens(
            token_a_amount,
            token_b_amount,
            pool.reserve_a,
            pool.reserve_b,
            pool.total_liquidity,
          )

        // LP tokens must be positive
        let lp_valid = lp_tokens > 0

        // New reserves must maintain ratio (within some tolerance)
        let new_a = pool.reserve_a + token_a_amount
        let new_b = pool.reserve_b + token_b_amount

        // Check ratio: a_amount * reserve_b should be close to b_amount * reserve_a
        let ratio_diff =
          token_a_amount * pool.reserve_b - token_b_amount * pool.reserve_a
        let abs_diff =
          if ratio_diff < 0 {
            -ratio_diff
          } else {
            ratio_diff
          }
        // Allow 1% tolerance
        let tolerance = pool.reserve_a * pool.reserve_b / 100
        let ratio_ok = abs_diff <= tolerance

        amounts_valid && lp_valid && ratio_ok && new_a > pool.reserve_a && new_b > pool.reserve_b
      }
      RemoveLiquidity { lp_amount, signers } -> {
        // Must have signer
        expect True = list.length(signers) > 0

        // LP amount must be positive and not exceed total
        let amount_valid =
          lp_amount > 0 && lp_amount <= pool.total_liquidity

        // Calculate tokens to return
        let token_a_return =
          lp_amount * pool.reserve_a / pool.total_liquidity
        let token_b_return =
          lp_amount * pool.reserve_b / pool.total_liquidity

        // Must return positive amounts
        let returns_valid = token_a_return > 0 && token_b_return > 0

        // New reserves after removal
        let new_reserve_a = pool.reserve_a - token_a_return
        let new_reserve_b = pool.reserve_b - token_b_return
        let reserves_positive = new_reserve_a > 0 && new_reserve_b > 0

        amount_valid && returns_valid && reserves_positive
      }
      UpdateFee { new_numerator, new_denominator, signers } -> {
        // Only admin can update
        let admin_signed = is_signed_by(signers, pool.admin)

        // New fee must be valid
        let fee_valid = validate_fee_params(new_numerator, new_denominator)

        // Fee can't be too high (max 10%)
        let not_too_high = new_numerator * 10 <= new_denominator

        admin_signed && fee_valid && not_too_high
      }
    }
  }
}
