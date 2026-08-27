{.push overflowChecks: off.}

var a: int64 = 0
var b: int64 = 1

for i in 1..100:
  echo "F(", i - 1, ") = ", a
  let next = a + b
  a = b
  b = next

{.pop.}

