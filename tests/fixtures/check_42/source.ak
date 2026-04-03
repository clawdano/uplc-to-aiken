/// A validator that checks if redeemer equals 42
validator check_42 {
  spend(_datum: Option<Data>, redeemer: Data, _output_ref: Data, _tx: Data) {
    expect n: Int = redeemer
    n == 42
  }
}
