---
name: kru
description: Manage tasks and buckets with the `kru` CLI. Use when asked to add, list, find, update, complete, move or remove tasks, record context or longer markdown notes on a task, manage buckets, or change kru's settings. Always call kru with --json and parse the result.
---

# Using kru from an agent

`kru` is a task manager: **tasks** live in **buckets**. This guide is for
agents (LLMs, scripts) driving it from a shell. Humans can read it too.

## Rules

1. **Always pass `--json`.** Text output is for people and may change; JSON
   is the interface. `--json` works anywhere on the command line
   (`kru task list --json` or `kru --json task list`).
2. **Never run bare `kru`.** With no command it opens an interactive TUI and
   blocks. `kru --json` with no command is an error, not the TUI.
3. **Check the exit code.** `0` = success, JSON on stdout. Non-zero = failure,
   nothing on stdout, a one-line message on stderr starting `Error:` (exit
   `1`) or `error:` (exit `2`, bad arguments).
4. **Refer to tasks by `id`, not `num`.** Every command that takes a task
   accepts either. `num` is only a position: tasks are numbered bucket by
   bucket, so any `add`, `move` or `remove` can shift other tasks' numbers.
   `id` never changes. Take it from the JSON (`.id`) and pass it back as is.
5. **Use `--all` to see done tasks.** `task list` hides them by default.
6. **Add context, don't overwrite it.** Use `task ctx <task> -a <text>` to
   record short notes. Replacing context discards what was there, so it
   needs `--force` (an agent can't answer the confirmation prompt).
7. **For long or rich notes, use the task's markdown file.** Get its path
   with `task md <task> --json` (this creates it if needed, never opens an
   editor) and read or write the file directly.

## Setup

```sh
cargo install --path .        # from the repo; installs `kru` on PATH
kru path --json               # {"path": "..."}: where data is stored
```

Data lives in the user's cache directory unless the `dir` setting says
otherwise (see [Settings](#settings)). To work on a throwaway copy (tests,
dry runs), point `KRU_DIR` at another directory; it overrides the setting:

```sh
export KRU_DIR="$(mktemp -d)"
```

## JSON shapes

### Task

Every command that returns a task returns this object. Keys after `bucket`
are the task's fields, in display order; new fields may appear in future
versions, so ignore keys you don't know.

```json
{
  "num": 1,
  "bucket": "home",
  "description": "Pay rent",
  "ctx": "Landlord: Sam\nPay by bank transfer",
  "priority": "high",
  "status": "todo",
  "due": "2026-10-12T11:30:00Z",
  "completed": null,
  "created": "2026-10-10T12:49:45.711884Z",
  "edited": "2026-10-10T12:49:45.711884Z",
  "mdpath": "notes/01a125dc-bcef-7784-8fc5-17784df85dec.md",
  "id": "01a125dc-bcef-7784-8fc5-17784df85dec"
}
```

| Key | Type | Notes |
|---|---|---|
| `num` | integer | Position for people; can shift, see rule 4 |
| `bucket` | string | Bucket name |
| `description` | string | |
| `ctx` | string | Extra context; may contain `\n`; `""` if none |
| `priority` | `"low"` \| `"medium"` \| `"high"` | |
| `status` | `"todo"` \| `"in-progress"` \| `"done"` | |
| `due` | RFC 3339 UTC string or `null` | |
| `completed` | RFC 3339 UTC string or `null` | Set when marked done |
| `created`, `edited` | RFC 3339 UTC string | |
| `mdpath` | string | Attached markdown file, relative to the store directory; `""` if none. `task md --json` gives the absolute path |
| `id` | UUID string | Stable for the task's lifetime; use it to refer to the task |

### Bucket

```json
{"name": "home", "id": "01a125dc-…", "active": true, "open": 2, "done": 1}
```

`active` marks the bucket `task add` uses when no `--bucket` is given.

### Change

```json
{"field": "priority", "from": "low", "to": "high"}
```

`from`/`to` use the same types as the task's field (times as RFC 3339 or
`null`).

## Commands

`<task>` is a task's `id` (recommended), a unique start of its id, or its
`num`. A value made only of digits is always read as a `num`.

| Command | Returns |
|---|---|
| `kru task list [filters] --json` | array of tasks (`[]` if none) |
| `kru task show <task> --json` | task |
| `kru task add -d <text> [field flags] [-b <bucket>] --json` | the new task |
| `kru task mark <task> <field flags> --json` | `{"task": task, "changes": [change, …]}` |
| `kru task ctx <task> --json` | task (read its `ctx`) |
| `kru task ctx <task> -a <text> --json` | `{"task": task, "changes": [change, …]}`; adds `text` as a new line |
| `kru task ctx <task> <text> --force --json` | same; replaces the whole context (`""` clears it) |
| `kru task md <task> --json` | `{"task": task, "path": "/abs/…/notes/<id>.md", "created": bool}`; attaches the file if needed |
| `kru task md <task> --remove --force --json` | `{"task": task, "removed": "/abs/…"}`; deletes the file |
| `kru task done <task>… --json` | array of the updated tasks |
| `kru task move <task> -b <bucket> --json` | the task, with its new `num` |
| `kru task remove <task>… --json` | array of the removed tasks, as they were |
| `kru bucket list --json` | array of buckets |
| `kru bucket add <name> --json` | bucket |
| `kru bucket use <name> --json` | bucket (now active) |
| `kru bucket rename <name> <new-name> --json` | bucket |
| `kru bucket remove <name> [--force] --json` | the removed bucket |
| `kru prune [--days N] --json` | `{"pruned": N}` |
| `kru path --json` | `{"path": "…"}`: the store directory |
| `kru config --json` | every setting, see [Settings](#settings) |
| `kru config get <key> --json` | one setting |
| `kru config set <key> <value> --json` / `unset <key>` | the setting after the change |

Aliases: `task ls`, `task edit` (= `mark`), `task mv`, `task rm`,
`bucket ls`, `bucket rm`.

### Field flags (`task add`, `task mark`)

| Flag | Values |
|---|---|
| `-d`, `--description` | text (required for `add`) |
| `-c`, `--ctx` | text, may contain newlines; replaces the context |
| `-p`, `--priority` | `low`, `medium`, `high` (also `l`, `m`/`med`, `h`) |
| `-s`, `--status` | `todo`, `in-progress`, `done` (also `doing`, `wip`) |
| `--due` | `2026-10-12`, `"2026-10-12 17:00"`, `today`, `tomorrow`, `fri`/`friday`, `3d`, `2h`, `1w`, or `none` to clear |

Values are case-insensitive. Dates are read in the user's local time zone;
a date without a time means the end of that day. `kru task <add|mark> --help`
always lists the current fields.

### List filters (`task list`)

| Flag | Effect |
|---|---|
| `-b`, `--bucket <name>` | only this bucket |
| `-s`, `--status <status>` | only this status (includes done tasks if `done`) |
| `-p`, `--priority <priority>` | only this priority |
| `-q`, `--query <text>` | description contains text (case-insensitive) |
| `-a`, `--all` | include done tasks |

Results are ordered most urgent first: open before done, then earliest due
date (no due date last), then higher priority.

## Behaviour to rely on

- **Buckets must exist.** `task add` fails if the bucket doesn't exist, or if
  no `--bucket` is given and no bucket is active. Bucket names are matched
  case-insensitively.
- **Edits are all-or-nothing.** If any flag in a `task mark` is invalid,
  nothing changes. Setting a value the task already has is a no-op:
  `changes` is `[]` and the task is not touched.
- **Input is cleaned.** Control characters are dropped and runs of spaces
  become one. The description is a single line: newlines become spaces, it
  can't be empty, and it's at most 500 characters. Context keeps its line
  breaks (blank lines at the ends are dropped, runs of blank lines become
  one), may be empty, and is at most 10,000 characters.
- **Markdown files live with the data.** `task md` creates
  `<store dir>/notes/<task id>.md` (starting with the description as a
  heading) and links it to the task. Removing or pruning the task deletes
  the file. Deleting it on its own needs `--remove --force`.
- **Context replacement is guarded.** `task ctx <task> <text>` on a task
  that already has context fails without `--force` when not run in a
  terminal (or with `--json`). Appending (`-a`) never needs it. Note that
  `task mark --ctx` replaces without asking, like any other field flag.
- **Done tasks are pruned.** Tasks done more than 7 days ago are deleted
  automatically whenever kru starts.
- **Removing a bucket with open tasks needs `--force`** and deletes its
  tasks.

## Recipes

```sh
# Set up a bucket and add a task to it, keeping its id.
kru bucket add work --json
id=$(kru task add -d "Write quarterly report" -p high --due fri -b work --json | jq -r '.id')

# Act on it later by id, whatever happened to the numbering.
kru task mark "$id" -s in-progress --json
kru task show "$id" --json

# Open tasks due in the next 3 days, soonest first.
cutoff=$(date -u -v+3d +%Y-%m-%dT%H:%M:%SZ 2>/dev/null || date -u -d '+3 days' +%Y-%m-%dT%H:%M:%SZ)
kru task list --json | jq --arg c "$cutoff" '[.[] | select(.due != null and .due <= $c)]'

# Overdue tasks.
now=$(date -u +%Y-%m-%dT%H:%M:%SZ)
kru task list --json | jq --arg now "$now" '[.[] | select(.due != null and .due < $now)]'

# Find a task by description, then mark it done.
id=$(kru task list -q "flights" --json | jq -r '.[0].id')
kru task done "$id" --json

# Change several fields at once and see what changed.
kru task mark "$id" -p medium -s in-progress --due none --json | jq '.changes'

# Complete every overdue task in one call (-r: do nothing if none).
kru task list --json | jq -r --arg now "$now" '.[] | select(.due != null and .due < $now) | .id' \
  | xargs -r kru task done --json

# Record notes as you go; each -a adds a line.
kru task ctx "$id" -a "Draft shared with Priya" --json
kru task ctx "$id" -a "Numbers: https://example.com/q3" --json
kru task show "$id" --json | jq -r '.ctx'

# Replace the context entirely (agents must pass --force).
kru task ctx "$id" "Final version sent" --force --json

# Keep long notes (code, lists, links) in the task's markdown file.
md=$(kru task md "$id" --json | jq -r '.path')
cat >> "$md" <<'EOF'
## Findings
- The report template lives in `templates/q3.md`
EOF

# Count open tasks per bucket.
kru bucket list --json | jq 'map({(.name): .open}) | add'
```

ISO 8601 UTC timestamps compare correctly as strings, which is why the `jq`
date filters above work. The `date` fallback covers macOS (`-v`) and GNU
(`-d`).

## Settings

Settings live in a JSON file in the user's config folder (`kru config path`;
`KRU_CONFIG` points kru at another file). Don't change them unless asked.

| Key | Meaning | Effective value comes from |
|---|---|---|
| `dir` | where tasks are stored | `$KRU_DIR`, else this setting, else the cache folder |
| `editor` | command that opens files (`nvim`, `code --wait`) | this setting, else `$VISUAL`, else `$EDITOR`, else `vi` |

`kru config --json` returns, per key, what kru uses, where it came from
(`config`, `env:NAME` or `default`) and what the file says (`null` if unset):

```json
{"dir": {"value": "/Users/me/Library/Caches/kru", "source": "default", "set": null},
 "editor": {"value": "nvim", "source": "config", "set": "nvim"}}
```

Changing `dir` doesn't move existing data.

## Errors

| Message (stderr) | Exit | Fix |
|---|---|---|
| `Error: no bucket given and no active bucket set` | 1 | pass `-b <bucket>` or `kru bucket use <name>` |
| `Error: bucket \`x\` does not exist` | 1 | `kru bucket list --json` to see names, or `bucket add` |
| `Error: no task #N` | 1 | numbers shift; use the task's `id`, or re-read with `task list --all --json` |
| `Error: no task matches \`x\`` | 1 | check the id with `task list --all --json` |
| `Error: \`x\` matches N tasks; use more of the id` | 1 | pass the full `id` |
| `Error: description cannot be empty` | 1 | pass `-d <text>` |
| `Error: invalid due date \`x\` (try …)` | 1 | use a format from the table above |
| `Error: bucket \`x\` still has N open task(s); use --force …` | 1 | move or finish the tasks, or pass `--force` |
| `Error: nothing to change; pass any of …` | 1 | give `task mark` at least one field flag |
| `Error: replacing the context needs confirmation; pass --force …` | 1 | use `-a` to add instead, or pass `--force` |
| `Error: …: written by a newer kru (file format N; …)` | 1 | the data was written by a newer kru; upgrade kru, don't edit the files |
| `Error: deleting the markdown file needs confirmation; pass --force …` | 1 | pass `--force` if deleting is intended |
| `Error: #N has no markdown file` | 1 | nothing to remove; `task md <task> --json` creates one |
| `Error: no config key \`x\` (expected dir or editor)` | 1 | use one of the keys in [Settings](#settings) |
| `error: invalid value 'x' for '--status …'` | 2 | bad argument; see `--help` |

## Example session

```sh
$ kru bucket add home --json
{"name": "home", "id": "01a125dc-…", "active": true, "open": 0, "done": 0}

$ id=$(kru task add -d "Pay rent" -p high --due tomorrow --json | jq -r '.id')

$ kru task show "$id" --json | jq '{num, due}'
{"num": 1, "due": "2026-10-11T18:29:59Z"}

$ kru task mark "$id" -s in-progress --json | jq '.changes'
[{"field": "status", "from": "todo", "to": "in-progress"}]

$ kru task done "$id" --json | jq '.[0] | {status, completed}'
{"status": "done", "completed": "2026-10-10T12:49:45.743686Z"}

$ kru task list --json
[]

$ kru task list --all --json | jq 'length'
1
```
