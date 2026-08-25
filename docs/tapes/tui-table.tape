# A sheet: navigate, resize a column, sort, filter, edit a cell.
Output docs/media/tui-table.gif
Set Shell zsh
Set FontSize 16
Set Width 1100
Set Height 420
Set Padding 12
Set TypingSpeed 60ms

Hide
Type "clear" Enter
Show
Type "vuwr examples/stock.csv" Enter
Sleep 2s

# move around
Down Down Right Right
Sleep 800ms

# widen the column under the cursor, then narrow it back
Type ">>>>"
Sleep 800ms
Type "<<"
Sleep 600ms

# sort by it, numerically
Type "s"
Sleep 1.5s

# filter to one warehouse
Type "&"
Type "Melbourne"
Enter
Sleep 2s
Type "r"
Sleep 1s

# edit a cell in place
Down Right
Type "i"
Sleep 500ms
Type "220"
Sleep 800ms
Enter
Sleep 1.5s
Type "q"
Sleep 1s
