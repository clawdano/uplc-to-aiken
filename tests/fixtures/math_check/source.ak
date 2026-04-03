/// A validator with if-else and arithmetic
validator math_check {
  spend(_datum: Option<Data>, redeemer: Data, _output_ref: Data, _tx: Data) {
    expect n: Int = redeemer
    if n > 0 {
      n * 2 == 84
    } else {
      False
    }
  }
}
