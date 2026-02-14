# VHS demo script for kr
# Install VHS: https://github.com/charmbracelet/vhs
# Run: vhs demo.tape
#
# Prerequisites: a working kubeconfig with accessible cluster

Output assets/demo.gif

Set FontSize 14
Set Width 1200
Set Height 700
Set Theme "Catppuccin Mocha"
Set TypingSpeed 80ms

# Start kr
Type "kr"
Enter
Sleep 4s

# Navigate pods
Type "j"
Sleep 400ms
Type "j"
Sleep 400ms
Type "j"
Sleep 600ms

# View pod logs (while pod is selected)
Type "l"
Sleep 3s

# Scroll logs up
Type "k"
Sleep 300ms
Type "k"
Sleep 300ms
Type "k"
Sleep 300ms

# Resume auto-follow
Type "G"
Sleep 2s

# Exit logs back to list
Escape
Sleep 500ms

# Filter pods
Type "/"
Sleep 300ms
Type "api"
Sleep 1500ms
Escape
Sleep 500ms

# Switch to Deployments tab
Tab
Sleep 1500ms

# Navigate deployments
Type "j"
Sleep 300ms
Type "j"
Sleep 500ms

# Switch to Secrets tab
Tab
Sleep 1500ms

# Go back to Pods
Tab
Sleep 1500ms

# Switch namespace
Type "n"
Sleep 1s
Type "j"
Sleep 300ms
Enter
Sleep 3s

# Navigate in new namespace
Type "j"
Sleep 300ms
Type "j"
Sleep 500ms

# Quit
Type "q"
Sleep 500ms
