# ghview

A terminal UI for browsing GitHub pull requests. Navigate sources, repos, and PRs with vim-style keys. Perform actions without leaving the terminal.

This project was slapped+slopped together over a weekend with my day job in mind. It works for my use case, but rough edges exist. I'll tweak it toward general usefulness in my free time. Contributions are welcome!

Requires the [`gh` CLI](https://cli.github.com/) authenticated (`gh auth login`).

A [Nerd Font](https://www.nerdfonts.com/) is required for icons (PR state, CI status, review status, language glyphs). Any patched Nerd Font works, set it as your terminal font.

## Installation

```sh
cargo install --path .
```

## Layout

Three columns, focus moves left/right:

```
┌─ Sources ──┬─ @you  updated ──┬─ owner/repo ────────────────────┐
│ @you       │  repo-a          │ #42 Fix the thing     ✓  ✓  ● 2 │
│ some-org   │  repo-b          │   @alice  3 days ago            │
│            │  repo-c          │ #41 Draft: new feature  DRAFT   │
│            │  ...             │   @you  1 week ago              │
└────────────┴──────────────────┴─────────────────────────────────┘
⚡4877/5000
```

The status bar shows GitHub API rate limit (⚡remaining/limit), color-coded green → yellow → red. The limit is 5000 requests/hour and resets on a rolling hourly window.

Run `ghview OWNER/REPO` to skip straight to a repo's workspace - the Sources and Repos columns aren't loaded and don't appear at all, so the content columns (PRs/Issues/Frontpage + detail) get the full width. `h`/Left won't back out of it since there's nothing to browse back to.

Run `ghview OWNER` (no slash) to skip straight to that owner's repo list instead - OWNER is resolved at launch via the GitHub API to figure out whether it's an org or a user account, then the Repos column loads for real. The Sources column is skipped, but Repos browsing works normally.

## Navigation

| Key | Action |
|-----|--------|
| `j` / `↓` | Move down |
| `k` / `↑` | Move up |
| `h` / `←` | Focus left |
| `l` / `→` / `Enter` | Focus right |
| `g` / `Home` | Jump to top |
| `G` / `End` | Jump to bottom |
| `/` | Filter current column |
| `S` | Cycle sort order |
| `r` | Refresh current column |
| `i` | Open repo's issues page in browser (Repos column) |
| `o` | Open in browser |
| `y` | Copy URL |
| `C` | Clone the current repo, or clone all repos in org/user (Sources, asks to confirm) |
| `?` | Show help |
| `q` | Quit |

Scrolling to the bottom of repos or PRs automatically loads the next page (50 items per page).

## Repo Workspace

Press `l` / `→` from the Repos column to enter the repo workspace. Use these keys to switch views (visible in the bottom tab bar):

| Key | View |
|-----|------|
| `f` | Frontpage (README and description) |
| `p` | Pull Requests |
| `i` | Issues |

In the Issues view, press `l` to open the issue body panel, `h` to go back.

## PR Actions

Available when focused on the PRs column:

| Key | Action |
|-----|--------|
| `v` | Approve |
| `m` | Merge (enables auto-merge; method set by `merge_method` config) |
| `C` | Checkout (interactive) |
| `c` | Comment (interactive) |
| `d` | View diff inline |
| `x` / `X` | Close / reopen |
| `W` | Mark ready (remove draft) |
| `b` | Dependabot commands (if PR is from dependabot) |

## Detail Panel

Press `l` / `→` / `Enter` while focused on the PRs column to enter the detail panel. Press `Tab` to switch between sections (Body / Checks). Press `h` / `←` to return to the PRs column.

### Checks

Available when the Checks section is focused in the detail panel:

| Key | Action |
|-----|--------|
| `o` | Open selected check run in browser |
| `O` | Open the PR in browser |
| `R` | Re-run the selected check |

Check runs refresh automatically every 30 seconds while a PR is selected and the Repo or Detail panel is focused.

## Configuration

Config file: `~/.config/ghview/config.toml` (respects `$XDG_CONFIG_HOME`). See [`config.example.toml`](config.example.toml) for all options with defaults.

```toml
[cache]
# Seconds before cached PR list is re-fetched. 0 = always fetch.
duration_secs = 600

[ui]
# UI tick rate in milliseconds.
tick_ms = 100
# Default repo sort: "updated" (most recently pushed) or "alpha".
repo_sort = "updated"
# Items per page when fetching lists. 0 = dynamic (~1.5× terminal height, clamped 10–50). Max 100.
# per_page = 0
# Merge method used by the m keybinding: "squash" (default), "merge", or "rebase".
# merge_method = "squash"
# Directory to cd into before running `gh pr checkout`. Supports ~.
# checkout_dir = "~/code"
# Directory repos are cloned into, as {clone_dir}/{owner}/{repo}. Supports ~.
# clone_dir = "~/code"
# Extra columns in the repos list. Default: ["stars"].
# Supported: "stars", "forks", "issues", "visibility", "last_push"
# repo_columns = ["stars"]
# Default view when entering a repo: "prs", "frontpage" (README + stats), or "issues".
# default_repo_view = "prs"
# Pre-fetch diff stats, check summary, and mergeable state for all PRs when the list loads.
# Costs 1–2 extra API calls per PR. Disable if you hit rate limits.
# prefetch_pr_details = true

[sources]
# Automatically fetch org memberships for the authenticated user.
auto_fetch_orgs = true
# Always include the authenticated user as a source.
include_self = true
# Additional orgs to always show.
orgs = ["my-org", "another-org"]
# Additional users to always show.
users = ["some-user"]
```

### Keybindings

Define custom keybindings per column. Each entry requires a `key` and one of `builtin` (invoke a built-in action by name) or `command` (run a shell command). Set `interactive = true` to suspend the TUI and run the command in the foreground.

See [`config.example.toml`](config.example.toml) for the full list of built-in action names and variable reference.

#### Universal

Active in every column. Checked before defaults, so they can override built-in key assignments.

```toml
[[keybindings.universal]]
key     = "Q"
builtin = "quit"
name    = "quit (shift-Q)"
```

#### Repos column

Variables: `{owner}`, `{org}`, `{repo}`, `{name}`, `{language}`, `{url}`

```toml
[[keybindings.repos]]
key         = "t"
name        = "clone & open shell"
command     = "gh repo clone {owner}/{repo} && cd {repo} && $SHELL"
interactive = true
```

#### PRs column

Variables: `{pr_number}`, `{owner}`, `{org}`, `{repo}`, `{author}`, `{head_ref}`, `{base_ref}`, `{url}`, `{title}`

```toml
[[keybindings.prs]]
key     = "R"
name    = "request changes"
command = "gh pr review {pr_number} -R {owner}/{repo} --request-changes -b ''"

[[keybindings.prs]]
key         = "e"
name        = "edit in $EDITOR"
command     = "gh pr checkout {pr_number} -R {owner}/{repo} && $EDITOR"
interactive = true
```

#### Checks section (detail panel)

Active when the Checks section of the detail panel is focused.

Variables: `{check_id}`, `{check_name}`, `{check_url}`, `{pr_number}`, `{owner}`, `{org}`, `{repo}`

```toml
[[keybindings.checks]]
key     = "L"
name    = "open check log"
command = "open {check_url}"
```

Key format: single character (`a`), uppercase (`A`), `ctrl+x`, or `alt+x`.

## Flags

```
OWNER/REPO    Open directly into a repo's workspace, skipping Sources/Repos browsing
OWNER         Open directly into an owner's repo list (org or user, auto-detected), skipping Sources browsing
--debug       Write debug logs to ./debug.log
```

## Notes

If pushing a branch that modifies `.github/workflows/` fails with "refusing to allow an OAuth App to create or update workflow ... without `workflow` scope", add the scope to your `gh` token:

```sh
gh auth refresh --scopes workflow
```

## Acknowledgements

Inspired by [gh-dash](https://github.com/dlvhdr/gh-dash).

Built with [ratatui](https://github.com/ratatui/ratatui).
