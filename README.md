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
- **Teleport support** — log into `tsh` clusters straight from the context picker, no `tsh kube login` beforehand
- **Namespace discovery** — finds your namespaces even without cluster-wide `list namespaces` permission
- **Describe & edit** — `kubectl describe` and `kubectl edit` in embedded views
- **RBAC-aware** — graceful handling of 403 Forbidden errors
- **Loading feedback** — animated spinner with elapsed time
- **Persistent state** — remembers namespaces and your last namespace per context across sessions

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

In the context picker, entries marked `⚡` are Teleport clusters you have access to but are not
logged into yet — see [Teleport](#teleport).

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

## Teleport

Entirely optional. Without `tsh` installed, kr behaves exactly as a plain kubeconfig client and
none of this applies.

When `tsh` is present and you have an active Teleport session, the context picker (`c`) also lists
clusters you have access to but have **not** run `tsh kube login` for. They appear as
`⚡ cluster-name (teleport)`. Selecting one runs the login for you — kr drops out of the TUI so MFA
prompts and browser redirects work normally — then switches to the new context. If your Teleport
session has expired, the picker offers `⚡ Log in to Teleport` instead.

kr never files access requests on your behalf: `tsh kube login` is always invoked with
`--disable-access-request`, since selecting a row in a list should not create a server-side
approval request.

## Namespaces

Some clusters grant access to individual namespaces without allowing `list namespaces` at the
cluster scope, which normally leaves the namespace picker empty. kr tries three sources in order:

1. the Kubernetes API
2. `kubectl get namespaces`
3. your Teleport role grants, read locally from `tsh status`

Source 3 is a union across all your Teleport roles, so kr narrows it to the namespaces that
actually work in the current cluster by asking the cluster what you are allowed to do in each one
(`SelfSubjectRulesReview`, which needs no special permission).

kr does not invent a namespace. If nothing is known yet, none is selected and no requests are made
— so you get an empty view instead of a confusing permission error. Once the list is known:

- exactly one namespace, or a confirmed `default` → selected automatically
- otherwise → the namespace picker opens

Your choice is remembered per context, so later switches land where you left off.

## Requirements

- Rust 1.75+ (to build from source)
- `kubectl` configured with a valid kubeconfig
- `kubectl` binary in PATH (for describe, edit, CLI mode)
- `tsh` in PATH — optional, only for [Teleport](#teleport) clusters

## Configuration

kr stores persistent state (namespace history and last namespace per context) in `kr/state.json`,
and TUI logs in `kr/kr.log`, under your platform's config directory:

| Platform | Directory |
|----------|-----------|
| Linux | `$XDG_CONFIG_HOME/kr` or `~/.config/kr` |
| macOS | `~/Library/Application Support/kr` |
| Windows | `%APPDATA%\kr` |

The log is rotated to `kr.log.1` at startup once it exceeds 16 MB.

## License

[MIT](LICENSE)
