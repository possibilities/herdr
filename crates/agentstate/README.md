# agentstate

Claude Code and Codex agent state detection, extracted from herdr for agent
development environments. The engine, the manifests, and the timing policy
are herdr's own source, compiled by path from the enclosing fork; this crate
adds a two-agent shim, a clock-driven multi-session tracker, and a JSONL
command line. It is maintained by the [herdx workshop](https://github.com/possibilities/herdx),
whose `/maintain` cycle rebuilds it on current herdr master.

## States

`idle`, `working`, `blocked`, `unknown`. herdr's `done` is a presentation
fact (idle and not yet viewed) and is not reproduced.

Claude Code and Codex are screen-manifest agents in herdr: their hooks report
only session identity, and state comes from the screen and terminal title.
herdr tried hook-driven state for Claude and reverted it (issue #198); a
hook cannot see a cancelled permission prompt or an escape interrupt.

## Input contract

Each snapshot is three strings.

- `screen`: the visible screen as text. Each row is the cell text with
  wide-glyph spacer cells skipped and trailing whitespace trimmed; rows are
  joined by newline; trailing blank rows are dropped; wrapped lines are not
  joined; the alternate screen is used when active. `tmux capture-pane -p`
  (without `-N`) with trailing blank rows removed matches this. herdr itself
  reads exactly `rows` rows ending at the last non-blank viewport row, which
  reaches into scrollback only when the bottom of the screen is blank.
- `osc_title`: the latest OSC 0/2 title the harness set. Empty when none.
- `osc_progress`: the latest OSC 9 payload (for example `4;3;`). Empty when
  unavailable; only Claude's low-priority idle rule reads it.

Pass empty strings, never placeholders, for unavailable values.

## Library

```rust
use agentstate::{Agent, AgentState, Tracker, SnapshotUpdate};

let detection = agentstate::detect(Agent::Claude, screen, osc_title, "");
let explain = agentstate::explain(Agent::Codex, screen, osc_title, "");
println!("{}", agentstate::explain_to_json_value(&explain));

let mut tracker = Tracker::new();
tracker.start("claude-1", Agent::Claude, now)?;               // publishes Unknown
tracker.snapshot("claude-1", None, SnapshotUpdate { screen, osc_title: Some(title), osc_progress: None }, now)?;
if let Some(deadline) = tracker.next_deadline() { /* sleep until deadline */ }
for t in tracker.tick(now) { /* publish t.session, t.state, t.rule */ }
```

The tracker owns herdr's time behavior with an explicit clock: a 3 s grace
after a session starts, a working-to-plain-idle hold that needs three 100 ms
rechecks or 700 ms, publish on state change only, and transcript viewers
that hold the previous state. Callers push snapshots on change; the tracker
re-evaluates the last snapshot from its own timer.

## Command line

```
agentstate explain --agent claude|codex --screen FILE [--osc-title T] [--osc-progress P]
agentstate track
agentstate --version
```

`explain` prints herdr's explain JSON: final state, matched rule, every
evaluated rule with region previews, and the fallback reason.

`track` is a long-lived process: JSONL in on stdin, JSONL out on stdout,
diagnostics on stderr, any number of sessions.

Input lines:

```json
{"session":"claude-1","agent":"claude","started":true}
{"session":"claude-1","agent":"claude","ts_ms":1725370000123,"screen":"...","osc_title":"⠂ project","osc_progress":null}
{"session":"claude-1","exited":true}
```

`started` is optional and starts the grace early. `agent` is required on
`started` and on a session's first snapshot, accepted on every line, and a
change of agent for a live session is an error. An absent `osc_title` or
`osc_progress` means unchanged; `null` means empty. `exited` drops the
session and its timers and emits nothing. A malformed line is reported on
stderr and skipped.

Output lines, one per state change:

```json
{"session":"claude-1","state":"unknown","rule":null,"ts_ms":1725370000123}
{"session":"claude-1","state":"working","rule":"osc_title_working","ts_ms":1725370000423}
```

`rule` is the manifest rule that decided the state, or
`default_known_agent_idle_fallback` when no rule matched.

## Overrides

A manifest at `$AGENTSTATE_CONFIG_DIR/agent-detection/<agent>.toml`
(default `~/.config/agentstate/agent-detection/`) replaces the bundled one,
with herdr's validation. Remote manifest fetching is not implemented.

## Tests

`cargo test -p agentstate`. Golden cases come from herdr's manifest tests;
consumer-contributed tmux captures live under `tests/fixtures/`.
