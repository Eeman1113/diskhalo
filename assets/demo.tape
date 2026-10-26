# Demo recording for diskhalo. Render with: vhs assets/demo.tape
Output assets/demo.gif

Set Shell "bash"
Set FontSize 15
Set Width 1300
Set Height 760
Set Padding 14
Set TypingSpeed 55ms
Set Theme { "name": "diskhalo", "background": "#0e0f14", "foreground": "#d6dae6", "cursor": "#7ac4ff", "black": "#0e0f14", "red": "#ff8a8a", "green": "#8ed8a0", "yellow": "#c98e5c", "blue": "#7ac4ff", "magenta": "#c678dd", "cyan": "#56b6c2", "white": "#d6dae6", "brightBlack": "#6c7488", "brightRed": "#ff8a8a", "brightGreen": "#8ed8a0", "brightYellow": "#c98e5c", "brightBlue": "#7ac4ff", "brightMagenta": "#c678dd", "brightCyan": "#56b6c2", "brightWhite": "#ffffff" }

Hide
Type "export PS1='\033[38;5;75m$\033[0m '" Enter
Type "export PATH=$PWD/target/release:$PATH" Enter
Type "clear" Enter
Show

Sleep 700ms
Type "diskhalo /opt/homebrew"
Sleep 400ms
Enter

# 507k files, 25.7 GB — the scan screen is visible for about two seconds
Sleep 3.5s

# Selecting a wedge lights it up across every ring it touches
Down@550ms 2
Sleep 1.3s
Up@550ms 1
Sleep 1.5s

# Drill into Cellar: 201 formulae, the shape this view is built for
Enter
Sleep 2.8s
Down@480ms 4
Sleep 1.5s

# Same scan, rectangles instead of rings
Tab
Sleep 3s
Down@480ms 4
Sleep 1.8s

# Toggle apparent vs on-disk, then back out
Type "a"
Sleep 2s
Backspace
Sleep 2s

Type "q"
Sleep 800ms
