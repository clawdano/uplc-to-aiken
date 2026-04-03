/// A validator with trace for debugging
validator traced {
  spend(_datum: Option<Data>, redeemer: Data, _output_ref: Data, _tx: Data) {
    expect n: Int = redeemer
    if n == 42 {
      trace @"correct answer!"
      True
    } else {
      trace @"wrong answer!"
      fail
    }
  }
}
