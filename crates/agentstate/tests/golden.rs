//! Detection parity cases. Screens and expectations are taken from herdr's
//! own manifest tests (`src/detect/manifest/tests.rs`) so a maintenance cycle
//! that changes a manifest changes these verdicts visibly. tmux captures
//! contributed by consumers live under `tests/fixtures/` and are checked by
//! `fixture_index_verdicts`.

use agentstate::{Agent, AgentState, DEFAULT_KNOWN_AGENT_IDLE_FALLBACK};

fn verdict(
    agent: Agent,
    screen: &str,
    title: &str,
    progress: &str,
) -> (AgentState, Option<String>) {
    let explain = agentstate::explain(agent, screen, title, progress);
    (explain.state, explain.matched_rule.map(|rule| rule.id))
}

#[test]
fn claude_idle_prompt_with_background_shell_is_idle() {
    let screen = concat!(
        "✻ Sautéed for 10s · 1 shell still running\n\n",
        "──────────────────────────────────────────────────────── WINDOWS ─\n",
        "❯\n",
        "────────────────────────────────────────────────────────────────\n",
        "  ⏵⏵ auto mode on · 1 shell · ← for agents                     /rc\n",
    );
    let explain = agentstate::explain(Agent::Claude, screen, "", "");
    assert_eq!(explain.state, AgentState::Idle);
    assert_eq!(
        explain.matched_rule.as_ref().map(|r| r.id.as_str()),
        Some("live_prompt_box")
    );
    assert!(explain.visible_idle);
    assert!(!explain.visible_working);
}

#[test]
fn claude_background_shell_without_foreground_evidence_is_idle_fallback() {
    let explain = agentstate::explain(
        Agent::Claude,
        "  ⏵⏵ auto mode on · 1 shell · ← for agents\n",
        "",
        "",
    );
    assert_eq!(explain.state, AgentState::Idle);
    assert_eq!(explain.matched_rule, None);
    assert_eq!(
        explain.fallback_reason.as_deref(),
        Some(DEFAULT_KNOWN_AGENT_IDLE_FALLBACK)
    );
}

#[test]
fn claude_live_turn_with_background_shell_remains_working() {
    let screen = concat!(
        "────────────────────────────────────────────────────────────────\n",
        "❯\n",
        "────────────────────────────────────────────────────────────────\n",
        "  ⏵⏵ auto mode on · 1 shell · esc to interrupt\n",
    );
    let explain = agentstate::explain(Agent::Claude, screen, "", "");
    assert_eq!(explain.state, AgentState::Working);
    assert_eq!(
        explain.matched_rule.as_ref().map(|r| r.id.as_str()),
        Some("live_turn_working")
    );
    assert!(explain.visible_working);
}

#[test]
fn claude_bash_permission_prompt_is_blocked() {
    let screen = concat!(
        "do you want to proceed?\n",
        "bash command: rm -rf /tmp/test\n",
        "❯ 1. Yes\n",
        "  2. No\n\n",
        "Esc to cancel · Tab to amend · ctrl+e to explain\n",
        "  ⏵⏵ auto mode on · 1 shell · ← for agents\n",
    );
    let explain = agentstate::explain(Agent::Claude, screen, "", "");
    assert_eq!(explain.state, AgentState::Blocked);
    assert_eq!(
        explain.matched_rule.as_ref().map(|r| r.id.as_str()),
        Some("bash_permission_prompt")
    );
    assert!(explain.visible_blocker);
}

#[test]
fn claude_osc_title_spinner_frames_are_working() {
    for title in [
        "⠂ project",
        "◐ Initial conversation with Claude",
        "◓ x",
        "◑ x",
        "◒ x",
    ] {
        assert_eq!(
            verdict(Agent::Claude, "", title, ""),
            (AgentState::Working, Some("osc_title_working".into())),
            "title {title:?}"
        );
    }
}

#[test]
fn claude_osc_title_static_prefix_is_idle() {
    assert_eq!(
        verdict(Agent::Claude, "", "✳ project", ""),
        (AgentState::Idle, Some("osc_title_idle".into()))
    );
}

#[test]
fn claude_osc_progress_idle_and_empty_inputs() {
    assert_eq!(
        verdict(Agent::Claude, "", "", "4;0;"),
        (AgentState::Idle, Some("osc_progress_idle".into()))
    );
    assert_eq!(verdict(Agent::Claude, "", "", ""), (AgentState::Idle, None));
}

#[test]
fn claude_blocker_screen_outranks_osc_idle_title() {
    let screen = concat!(
        "do you want to proceed?\n",
        "bash command: cargo test\n",
        "❯ 1. Yes\n",
        "  2. No\n",
        "Esc to cancel\n",
    );
    assert_eq!(
        verdict(Agent::Claude, screen, "✳ project", ""),
        (AgentState::Blocked, Some("bash_permission_prompt".into()))
    );
}

#[test]
fn claude_transcript_viewer_holds_state() {
    let screen = "some transcript line\nShowing detailed transcript · ctrl+o to toggle\n";
    let explain = agentstate::explain(Agent::Claude, screen, "", "");
    assert!(explain.skip_state_update);
    assert_eq!(explain.state, AgentState::Unknown);
}

#[test]
fn codex_osc_title_verdicts() {
    assert_eq!(
        verdict(Agent::Codex, "", "⠸ project", ""),
        (AgentState::Working, Some("osc_title_working".into()))
    );
    assert_eq!(
        verdict(Agent::Codex, "", "Action Required: project", ""),
        (AgentState::Blocked, Some("osc_title_blocked".into()))
    );
    assert_eq!(
        verdict(Agent::Codex, "", "project", ""),
        (AgentState::Idle, Some("osc_title_idle".into()))
    );
    assert_eq!(verdict(Agent::Codex, "", "", ""), (AgentState::Idle, None));
}

#[test]
fn codex_screen_working_fallback_handles_static_osc_title() {
    let screen = "• I’ll run it and wait for completion.\n\n\
        ◦ Working (1m 16s • esc to interrupt) · 1 background…\n\n\
        › Use /skills to list available skills\n\n\
        gpt-5.6-sol default · /work\n";
    assert_eq!(
        verdict(Agent::Codex, screen, "project", ""),
        (AgentState::Working, Some("screen_working_fallback".into()))
    );
}

#[test]
fn codex_osc_working_remains_preferred_over_screen_fallback() {
    let screen = "• Working (4s • esc to interrupt)\n\n\
        › Use /skills to list available skills\n\n\
        gpt-5.6-sol default · /work\n";
    assert_eq!(
        verdict(Agent::Codex, screen, "⠸ project", ""),
        (AgentState::Working, Some("osc_title_working".into()))
    );
}

#[test]
fn codex_screen_blocker_outranks_working_fallback() {
    let screen = "• Working (4s • esc to interrupt)\n\
        › 1. Yes, proceed\n\
        Press enter to confirm or esc to cancel\n";
    let explain = agentstate::explain(Agent::Codex, screen, "project", "");
    assert_eq!(explain.state, AgentState::Blocked);
    assert_eq!(
        explain.matched_rule.as_ref().map(|r| r.id.as_str()),
        Some("live_strong_blocker")
    );
    assert!(explain.visible_blocker);
    assert!(!explain.visible_working);
}

#[test]
fn codex_weak_blocker_depends_on_current_prompt() {
    assert_eq!(
        verdict(
            Agent::Codex,
            "do you want to continue? [y/n]\n",
            "project",
            ""
        ),
        (AgentState::Blocked, Some("weak_blocker".into()))
    );
    let screen = "• Working (4s • esc to interrupt)\n\
        do you want to continue? [y/n]\n\
        › Use /skills to list available skills\n";
    assert_eq!(
        verdict(Agent::Codex, screen, "project", ""),
        (AgentState::Working, Some("screen_working_fallback".into()))
    );
}

#[test]
fn codex_startup_update_requires_complete_live_chooser() {
    // herdr 2026.09.05.1 (`startup_update`): the Codex update chooser shown
    // at launch is a blocker only when every control of the chooser is live.
    let chooser = "Update available! 0.153.0 -> 9.8.7\n\
        Run bun add -g @openai/codex to update.\n\n\
        › 1. Update now\n\
          2. Skip until next version\n\n\
        Press enter to continue   \n";
    let wrapped = "✨ Update available! 0.153.0\n\n\
        Release notes: https://example\n\n\
        › 1. Update now (runs `npm\n\
             install -g\n\
             @openai/codex`)\n\
          2. Skip\n\
          3. Skip until next\n\
             version\n\n\
        Press enter to continue\n";
    for screen in [chooser, wrapped] {
        let explain = agentstate::explain(Agent::Codex, screen, "project", "");
        assert_eq!(explain.state, AgentState::Blocked);
        assert_eq!(
            explain.matched_rule.as_ref().map(|r| r.id.as_str()),
            Some("startup_update")
        );
        assert!(explain.visible_blocker);
    }
    for screen in [
        chooser.replace("Update now", "Install"),
        format!("{wrapped}\n› Ask Codex to do anything\n"),
    ] {
        let explain = agentstate::explain(Agent::Codex, &screen, "project", "");
        assert_eq!(explain.state, AgentState::Idle);
        assert_ne!(
            explain.matched_rule.as_ref().map(|r| r.id.as_str()),
            Some("startup_update")
        );
        assert!(!explain.visible_blocker);
    }
}

#[test]
fn codex_transcript_viewer_holds_state() {
    let screen = "› earlier prompt\n\
        transcript body\n\
        ↑/↓ to scroll   PgUp/PgDn to page   Home/End to jump   q to quit   Esc to edit prev\n";
    let explain = agentstate::explain(Agent::Codex, screen, "project", "");
    assert!(explain.skip_state_update);
}

#[test]
fn agent_labels_round_trip() {
    assert_eq!(agentstate::parse_agent_label("claude"), Some(Agent::Claude));
    assert_eq!(
        agentstate::parse_agent_label("Claude-Code"),
        Some(Agent::Claude)
    );
    assert_eq!(
        agentstate::parse_agent_label("/usr/local/bin/codex"),
        Some(Agent::Codex)
    );
    assert_eq!(agentstate::parse_agent_label("pi"), None);
    for agent in Agent::ALL {
        assert_eq!(
            agentstate::parse_agent_label(agentstate::agent_label(agent)),
            Some(agent)
        );
    }
}

/// Every fixture directory under `tests/fixtures/<agent>/` may hold
/// `<name>.txt` screens with a sibling `<name>.toml` declaring
/// `state`, optional `osc_title`, `osc_progress`, and optional `rule`.
#[test]
fn fixture_index_verdicts() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures");
    let mut checked = 0usize;
    for agent in Agent::ALL {
        let dir = root.join(agentstate::agent_label(agent));
        let Ok(entries) = std::fs::read_dir(&dir) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().and_then(|e| e.to_str()) != Some("txt") {
                continue;
            }
            let spec_path = path.with_extension("toml");
            let spec: toml::Value = toml::from_str(
                &std::fs::read_to_string(&spec_path)
                    .unwrap_or_else(|err| panic!("{}: {err}", spec_path.display())),
            )
            .unwrap_or_else(|err| panic!("{}: {err}", spec_path.display()));
            let screen = std::fs::read_to_string(&path).expect("fixture screen");
            let title = spec.get("osc_title").and_then(|v| v.as_str()).unwrap_or("");
            let progress = spec
                .get("osc_progress")
                .and_then(|v| v.as_str())
                .unwrap_or("");
            let expected = spec.get("state").and_then(|v| v.as_str()).expect("state");
            let explain = agentstate::explain(agent, &screen, title, progress);
            assert_eq!(
                agentstate::agent_state_label(explain.state),
                expected,
                "{}: expected {expected}, matched {:?}",
                path.display(),
                explain.matched_rule
            );
            if let Some(rule) = spec.get("rule").and_then(|v| v.as_str()) {
                assert_eq!(
                    explain.matched_rule.as_ref().map(|r| r.id.as_str()),
                    Some(rule),
                    "{}",
                    path.display()
                );
            }
            checked += 1;
        }
    }
    assert!(checked > 0, "no fixtures found under {}", root.display());
}
