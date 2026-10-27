# mdlive demo, recorded with vhs (https://github.com/charmbracelet/vhs)
# Usage: just demo
# vhs draws no terminal pictures, so the demo pages hold none.

Output doc/demo.gif

Set FontSize 14
Set Width 1300
Set Height 700
Set Theme "Rose Pine"
Set Framerate 10
Set TypingSpeed 120ms

# A folder opens in the file tree.
Hide
Type "./target/release/mdlive doc/demo"
Enter
Sleep 1500ms
Show

Sleep 2000ms
Enter
Sleep 2000ms

# Close the tree and open the contents
Type "-"
Sleep 800ms
Type "t"
Sleep 2000ms

# Next section, twice
Type "]]"
Sleep 1500ms
Type "]]"
Sleep 2000ms

# Search
Type "/period"
Sleep 500ms
Enter
Sleep 1500ms
Type "n"
Sleep 1500ms

# Open every fold, then the end of the page
Type "gg"
Sleep 500ms
Type "zR"
Sleep 1000ms
Type "G"
Sleep 2000ms

# Follow a link to another page, then back
Type "gg"
Sleep 500ms
Type "]l"
Sleep 1000ms
Enter
Sleep 2500ms
Ctrl+O
Sleep 1500ms

# Select lines and copy their Markdown source
Type "]]"
Sleep 800ms
Type "V"
Sleep 800ms
Type "jjjj"
Sleep 1000ms
Type "y"
Sleep 2000ms

# Every key
Type "g?"
Sleep 3000ms
Type "q"
Sleep 800ms
Type "q"
Sleep 300ms
