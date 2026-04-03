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
