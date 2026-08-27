proc addBig(a, b: string): string =
  # add two non-negative numbers given as decimal strings
  var carry = 0
  var i = a.high
  var j = b.high
  while i >= 0 or j >= 0 or carry > 0:
    let da = if i >= 0: ord(a[i]) - ord('0') else: 0
    let db = if j >= 0: ord(b[j]) - ord('0') else: 0
    s = da + db + carry
    result.add(chr(ord('0') + s mod 10))
    carry = s div 10
    dec i
    dec j
  result.reverse()

var a = "0"
var b = "1"
for i in 1..100:
  echo "F(", i - 1, ") = ", a
  let next = addBig(a, b)
  a = b
  b = next

