# kr

A fast, lightweight Kubernetes TUI built in Rust.

![demo](assets/demo.gif)

## Features

- **Three resource views** — Pods, Deployments, Secrets with Tab switching
- **Real-time updates** — watches resources via Kubernetes API (no polling)
- **Pod logs** — streaming log view with auto-follow, manual scroll, search, and visual-mode line copy
- **Shell access** — embedded interactive shell sessions inside pods
- **Port forwarding** — forward local ports to pods, manage multiple forwards with `p` / `P`
- **Secret decoding** — view decoded secret values, copy to clipboard
- **Deployment management** — scale replicas, rollout restart
- **Multi-select** — bulk delete pods/deployments with Space and Ctrl+A
- **Table sorting** — cycle sort columns with `o`, reverse with `O`
- **Wide view** — toggle extended columns (IP, Node, Image, etc.) with `w`
- **Fuzzy filter** — type `/` to filter resources by name
- **Context & namespace switching** — switch clusters and namespaces without leaving the TUI
- **Describe & edit** — `kubectl describe` and `kubectl edit` in embedded views
- **RBAC-aware** — graceful handling of 403 Forbidden errors
- **Loading feedback** — animated spinner with elapsed time
- **Persistent state** — remembers namespaces per context across sessions

## Installation

### From source

```bash
git clone https://github.com/crn4/kr.git
cd kr
cargo install --path .
```

### From GitHub Releases

Download the pre-built binary for your platform from the [Releases](https://github.com/crn4/kr/releases) page.

## Usage

```bash
# Launch TUI (uses current kubeconfig context)
kr

# Run a one-off kubectl command
kr -c "get pods -n kube-system"
```

## Keybindings

### Navigation

| Key | Action |
|-----|--------|
| `Tab` / `Shift+Tab` | Switch between Pods / Deployments / Secrets |
| `j` / `k` | Move up / down |
| `g` / `G` | Jump to top / bottom |
| `PgUp` / `PgDn` | Page scroll |
| `/` | Filter by name |
| `o` | Cycle sort column |
| `O` | Toggle sort direction (asc/desc) |
| `w` | Toggle wide view (Pods, Deployments) |
| `?` | Show context-aware help popup |
| `P` | Manage active port forwards |
| `Esc` | Clear filter / close modal / back |
| `q` | Quit |

### Cluster

| Key | Action |
|-----|--------|
| `c` | Switch context (cluster) |
| `n` | Switch namespace |

### Pods

| Key | Action |
|-----|--------|
| `l` | Stream logs |
| `s` | Open shell |
| `p` | Port forward (`8080:80` or `80`) |
| `d` | Describe |
| `e` | Edit |
| `f` | Filter by pod's status |
| `D` / `Delete` | Delete (with confirmation) |
| `Space` | Toggle select |
| `Ctrl+A` | Select / deselect all |

### Deployments

| Key | Action |
|-----|--------|
| `Enter` | Show deployment's pods (filtered) |
| `S` | Scale replicas |
| `r` | Rollout restart |
| `d` | Describe |
| `e` | Edit |
| `D` / `Delete` | Delete (with confirmation) |

### Secrets

| Key | Action |
|-----|--------|
| `Enter` / `x` | Decode and view |
| `r` | Reveal / hide values |
| `c` | Copy selected value to clipboard |

### Log View

| Key | Action |
|-----|--------|
| `j` / `k` | Scroll |
| `g` | Jump to top |
| `G` | Resume auto-follow |
| `/` | Search |
| `n` / `N` | Next / previous search match |
| `V` | Enter visual selection mode |
| `q` / `Esc` | Exit |

### Log Visual Select

Press `V` in the log view to select one or more lines to copy.

| Key | Action |
|-----|--------|
| `j` / `k` | Extend selection up / down |
| `PgUp` / `PgDn` | Extend by page |
| `g` / `G` | Jump to top / bottom |
| `y` / `Enter` | Copy selection to clipboard and exit |
| `V` / `Esc` / `q` | Cancel |

### Shell

| Key | Action |
|-----|--------|
| `Ctrl+Q` | Close shell session |
| All other keys | Forwarded to the shell |

### Port Forward List

| Key | Action |
|-----|--------|
| `j` / `k` | Navigate |
| `d` / `Delete` | Stop selected forward |
| `Esc` | Close |

## Requirements

- Rust 1.75+ (to build from source)
- `kubectl` configured with a valid kubeconfig
- `kubectl` binary in PATH (for describe, edit, CLI mode)

## Configuration

kr stores persistent state (namespace history per context) in:

```
~/.config/kr/state.json
```

Logs (TUI mode) are written to:

```
~/.config/kr/kr.log
```

## License

[MIT](LICENSE)
