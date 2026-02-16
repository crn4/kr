# VHS demo script for kr
# Install VHS: https://github.com/charmbracelet/vhs
# Run: vhs assets/demo.tape
#
# Prerequisites: a working kubeconfig with accessible cluster

Output assets/demo.gif

Set FontSize 14
Set Width 1200
Set Height 700
Set Theme "Catppuccin Mocha"
Set TypingSpeed 50ms

# Start kr
Type "kr"
Enter
Sleep 3s

# Navigate pods
Type "j"
Sleep 200ms
Type "j"
Sleep 200ms
Type "j"
Sleep 300ms

# View pod logs
Type "l"
Sleep 2s

# Scroll logs up
Type "k"
Sleep 150ms
Type "k"
Sleep 150ms
Type "k"
Sleep 150ms

# Resume auto-follow
Type "G"
Sleep 1s

# Exit logs
Escape
Sleep 300ms

# Filter pods
Type "/"
Sleep 200ms
Type "api"
Sleep 800ms
Escape
Sleep 300ms

# Switch to Deployments tab
Tab
Sleep 1s

# Navigate deployments
Type "j"
Sleep 200ms
Type "j"
Sleep 300ms

# Switch to Secrets tab
Tab
Sleep 1s

# Navigate secrets
Type "j"
Sleep 200ms
Type "j"
Sleep 200ms

# Decode secret
Enter
Sleep 1s

# Reveal secret values
Type "r"
Sleep 1s

# Navigate secret keys
Type "j"
Sleep 200ms
Type "j"
Sleep 200ms

# Copy secret value to clipboard
Type "c"
Sleep 1s

# Close secret modal
Escape
Sleep 300ms

# Go back to Pods
Tab
Sleep 1s

# Switch namespace
Type "n"
Sleep 600ms
Type "j"
Sleep 200ms
Enter
Sleep 2s

# Navigate in new namespace
Type "j"
Sleep 200ms
Type "j"
Sleep 300ms

# Quit
Type "q"
Sleep 300ms
