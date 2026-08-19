# cloop

**Agentic loops for Claude Code.** A tiny, zero-dependency Rust CLI that wraps
`claude --print` in a configurable loop — run it, check whether you're done, run
it again with feedback, until the tests pass, a marker appears, or you hit the
iteration cap.

![cloop demo](cloop-demo.gif)

## Why

Claude Code's headless mode (`claude --print`) is a single batch turn: one
prompt in, one result out, and an exit code you can branch on. On its own that's
a one-shot. The interesting behavior comes from putting it in a loop — let the
model work, run a check, feed the result back, and let it try again. That outer
loop is the thing you actually want to reuse, and it's the thing nobody wants to
re-hand-roll as a throwaway bash script every time.

`cloop` is that loop, made reusable, inspectable, and saveable. You answer a few
questions once, it saves a small TOML file, and from then on it's `cloop run
<name>`. It fits naturally alongside the rest of the turbo-flow toolchain.

## Install

Requires a Rust toolchain (MSRV **1.70**) and the `claude` CLI on your `PATH`.

```bash
# from a clone of this repo
cargo install --path .

# or just build the binary
cargo build --release   # -> target/release/cloop
```

## Quickstart

```bash
cloop                 # run the wizard, then optionally start the loop
```

The wizard walks you through: a name, the task, the working directory, the
model, a stop condition, the iteration cap, permissions, and (optionally) the
per-iteration prompt. It saves to `~/.config/cloop/loops/<name>.toml`.

A classic "keep going until the tests pass" loop:

```bash
cloop new
# name:         fix-tests
# task:         Make `cargo test` pass. Fix the failing parser cases.
# stop:         command  →  cargo test
# max iters:    25
cloop run fix-tests
```

Each iteration, `cloop` runs Claude, then runs your check command. The moment
the check exits `0`, the loop stops. If the cap is hit first, it exits non-zero
— handy in CI.

## Commands

| Command | Does |
|---|---|
| `cloop` | Run the wizard, then optionally start the loop |
| `cloop new` | Create and save a loop (no auto-run) |
| `cloop run [name]` | Run a saved loop (prompts to pick if no name) |
| `cloop list` | List saved loops |
| `cloop show <name>` | Print a loop's settings and task |
| `cloop export <name>` | Print an equivalent standalone bash script |
| `cloop edit <name>` | Open the loop file in `$EDITOR` (re-validates on save) |
| `cloop rm <name>` | Delete a saved loop |

### Run flags

```
-n, --dry-run        show the exact commands without executing them
-y, --yes            skip the confirmation prompt
    --max <n>        override the iteration cap for this run
    --config <path>  run a loop file directly, without saving it
```

## Stop conditions

- **command** — loop until a shell command exits `0`. The command's combined
  output is fed back into the next prompt. Best for test/lint/build loops.
- **marker** — loop until Claude prints a marker string (default `LOOP_DONE`)
  on its own line. Best for open-ended tasks the model decides are finished.
- **iterations** — run a fixed number of times, no early exit.

## Configuration

Loops are plain TOML you can read, diff, and edit by hand:

```toml
# cloop loop definition
name = "fix-tests"

task = """
Make `cargo test` pass. Focus on the failing parser cases.
"""

workdir = "."
model = "sonnet"

stop_kind = "command"
stop_command = "cargo test"
stop_marker = "LOOP_DONE"

max_iterations = 25
carry_context = true

permission_mode = ""
skip_permissions = false

max_turns = 0
allowed_tools = ""
delay_secs = 0
extra_args = ""

iter_prompt = """
Iteration {iteration} of {max}.

The check command `{stop_command}` is still failing. Its latest output was:

{check_output}

Diagnose why it is failing and fix it. Make concrete edits — do not just
describe the problem. When you believe it is resolved, stop.
"""
```

Override the storage location with `$CLOOP_DIR` (otherwise
`$XDG_CONFIG_HOME/cloop` or `~/.config/cloop`).

### Per-iteration prompt placeholders

`{iteration}` `{max}` `{task}` `{stop_command}` `{stop_marker}` `{check_output}`

Iteration 1 always uses your raw `task`. From iteration 2 on, the template
above is rendered with the placeholders filled in.

## How it maps to Claude Code

`cloop` doesn't reimplement anything — it just composes the CLI:

| cloop setting | Claude Code flag |
|---|---|
| (always) | `--print` |
| `carry_context` + iteration > 1 | `--continue` |
| `model` | `--model <name>` |
| `permission_mode` | `--permission-mode <mode>` |
| `skip_permissions` | `--dangerously-skip-permissions` |
| `max_turns` | `--max-turns <n>` |
| `allowed_tools` | `--allowedTools "<list>"` |
| `extra_args` | passed through verbatim |
| the prompt | trailing positional argument |

## Exporting to bash

`cloop export <name> > loop.sh` writes a standalone, dependency-free shell
script that approximates the same loop — useful for CI or for sharing a loop
where installing `cloop` isn't worth it. Complex permission or tool specs may
need a little manual quoting in the generated `FLAGS` line; the script says so
in a comment.

## A word on permissions

`bypassPermissions` / `--dangerously-skip-permissions` lets Claude act without
asking. In an unattended loop that means it can run many commands in a row with
no human in the path. Only use it inside a container or VM you're comfortable
handing the keys to. `cloop` shows a red warning and defaults the run prompt to
**No** whenever a loop has it enabled.

## License

MIT © 2026 Marcus Patman
