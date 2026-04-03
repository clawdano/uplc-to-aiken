/// A simple always-true validator
validator always_true {
  spend(_datum: Option<Data>, _redeemer: Data, _output_ref: Data, _tx: Data) {
    True
  }
}
