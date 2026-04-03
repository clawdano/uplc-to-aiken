/// A validator with pattern matching on Option
validator option_check {
  spend(datum: Option<Data>, _redeemer: Data, _output_ref: Data, _tx: Data) {
    when datum is {
      Some(_d) -> True
      None -> False
    }
  }
}
