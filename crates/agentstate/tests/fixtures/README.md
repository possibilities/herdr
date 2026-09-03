# Screen fixtures

One `<name>.txt` per captured screen, with a sibling `<name>.toml`:

```toml
state = "blocked"            # required: idle | working | blocked | unknown
rule = "bash_permission_prompt"   # optional: the rule expected to decide it
osc_title = "⠂ project"      # optional: pane title at capture time
osc_progress = "4;3;"        # optional
```

Screens are plain text as `tmux capture-pane -p` prints them, trailing blank
rows trimmed. Directories are named by agent label (`claude`, `codex`).
`tests/golden.rs` checks every fixture.
