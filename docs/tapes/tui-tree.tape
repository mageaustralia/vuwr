# XML: the tree, expanding nodes, and the larger editor for a long value.
Output docs/media/tui-tree.gif
Set Shell zsh
Set FontSize 16
Set Width 1100
Set Height 460
Set Padding 12
Set TypingSpeed 60ms

Hide
Type "clear" Enter
Show
Type "vuwr examples/products.xml" Enter
Sleep 2s

# right arrow opens a node, so you can walk down and in
Right
Sleep 700ms
Down Down
Sleep 500ms
Right
Sleep 900ms
Down Down Down
Sleep 1s

# a description is escaped markup: the larger editor shows it decoded
Type "V"
Sleep 2s
Type "V"
Sleep 500ms

# and the same document as a table
Type "1"
Sleep 2s
Right Right
Sleep 1s

# text view, with the value the cursor is in marked out
Type "3"
Sleep 2s
Down Down Down Down Down Down
Sleep 2s
Type "q"
Sleep 1s
