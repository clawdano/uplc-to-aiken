validator multi_condition {
  spend(_datum: Option<Data>, redeemer: Data, _output_ref: Data, _tx: Data) {
    expect n: Int = redeemer
    let x = n * 2
    let y = x + 1
    x > 0 && y < 100
  }
}
