type Action {
  Mint
  Burn
  Transfer { to: ByteArray, amount: Int }
}

validator token_policy {
  spend(datum: Option<Data>, redeemer: Data, _output_ref: Data, _tx: Data) {
    expect Some(raw_datum) = datum
    expect action: Action = redeemer
    when action is {
      Mint -> True
      Burn -> False
      Transfer { to, amount } -> amount > 0
    }
  }
}
