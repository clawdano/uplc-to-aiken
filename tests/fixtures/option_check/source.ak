/// A simple always-true validator
validator always_true {
  spend(_datum: Option<Data>, _redeemer: Data, _output_ref: Data, _tx: Data) {
    True
  }
}

/// A validator that checks if redeemer equals 42
validator check_42 {
  spend(_datum: Option<Data>, redeemer: Data, _output_ref: Data, _tx: Data) {
    expect n: Int = redeemer
    n == 42
  }
}

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

/// A validator with pattern matching on Option
validator option_check {
  spend(datum: Option<Data>, _redeemer: Data, _output_ref: Data, _tx: Data) {
    when datum is {
      Some(_d) -> True
      None -> False
    }
  }
}

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
