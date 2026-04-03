use aiken/collection/list

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

fn add(a: Int, b: Int) -> Int {
  a + b
}

validator with_helper {
  spend(_datum: Option<Data>, redeemer: Data, _output_ref: Data, _tx: Data) {
    expect n: Int = redeemer
    let result = add(n, 10)
    result > 42
  }
}

validator multi_condition {
  spend(_datum: Option<Data>, redeemer: Data, _output_ref: Data, _tx: Data) {
    expect n: Int = redeemer
    let x = n * 2
    let y = x + 1
    x > 0 && y < 100
  }
}
