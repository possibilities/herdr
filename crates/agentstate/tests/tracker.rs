//! Tracker timing parity with herdr's pane loop, driven by a fake clock.

use std::time::{Duration, Instant};

use agentstate::{Agent, AgentState, SnapshotUpdate, Tracker, Transition};

const CLAUDE_IDLE_PROMPT: &str = concat!(
    "────────────────────────────────────────────────────────────────\n",
    "❯\n",
    "────────────────────────────────────────────────────────────────\n",
    "  ⏵⏵ auto mode on · ← for agents\n",
);

const CLAUDE_WORKING: &str = concat!(
    "────────────────────────────────────────────────────────────────\n",
    "❯\n",
    "────────────────────────────────────────────────────────────────\n",
    "  ⏵⏵ auto mode on · esc to interrupt\n",
);

const CLAUDE_BLOCKED: &str = concat!(
    "do you want to proceed?\n",
    "bash command: rm -rf /tmp/test\n",
    "❯ 1. Yes\n",
    "  2. No\n\n",
    "Esc to cancel · Tab to amend · ctrl+e to explain\n",
);

/// A screen no Claude rule matches: herdr's plain idle fallback.
const CLAUDE_PLAIN: &str = "compiling...\ndone\n";

fn snap(screen: &str) -> SnapshotUpdate {
    SnapshotUpdate {
        screen: screen.to_string(),
        osc_title: None,
        osc_progress: None,
    }
}

fn states(transitions: &[Transition]) -> Vec<AgentState> {
    transitions.iter().map(|t| t.state).collect()
}

#[test]
fn start_publishes_unknown_and_grace_holds_classification() {
    let t0 = Instant::now();
    let mut tracker = Tracker::new();
    let out = tracker.start("s", Agent::Claude, t0).unwrap();
    assert_eq!(states(&out), vec![AgentState::Unknown]);
    assert_eq!(out[0].rule, None);

    // During the 3 s grace a screen is held, not classified.
    let out = tracker
        .snapshot("s", None, snap(CLAUDE_WORKING), t0 + Duration::from_secs(1))
        .unwrap();
    assert!(out.is_empty());
    assert_eq!(tracker.next_deadline(), Some(t0 + Duration::from_secs(3)));

    // Grace expiry classifies the held screen immediately.
    let out = tracker.tick(t0 + Duration::from_secs(3));
    assert_eq!(states(&out), vec![AgentState::Working]);
    assert_eq!(out[0].rule.as_deref(), Some("live_turn_working"));
    assert_eq!(tracker.next_deadline(), None);
}

#[test]
fn first_snapshot_creates_session_and_needs_agent() {
    let t0 = Instant::now();
    let mut tracker = Tracker::new();
    assert!(tracker
        .snapshot("s", None, snap(CLAUDE_WORKING), t0)
        .is_err());
    let out = tracker
        .snapshot("s", Some(Agent::Codex), snap("x"), t0)
        .unwrap();
    assert_eq!(states(&out), vec![AgentState::Unknown]);
    assert!(tracker
        .snapshot("s", Some(Agent::Claude), snap("x"), t0)
        .is_err());
    assert_eq!(tracker.agent("s"), Some(Agent::Codex));
}

fn working_session(t0: Instant) -> Tracker {
    let mut tracker = Tracker::new();
    tracker.start("s", Agent::Claude, t0).unwrap();
    tracker
        .snapshot("s", None, snap(CLAUDE_WORKING), t0 + Duration::from_secs(1))
        .unwrap();
    let out = tracker.tick(t0 + Duration::from_secs(3));
    assert_eq!(states(&out), vec![AgentState::Working]);
    tracker
}

#[test]
fn working_to_plain_idle_needs_three_rechecks_from_own_timer() {
    let t0 = Instant::now();
    let mut tracker = working_session(t0);
    let t = t0 + Duration::from_secs(4);
    let out = tracker.snapshot("s", None, snap(CLAUDE_PLAIN), t).unwrap();
    assert!(out.is_empty(), "plain idle is held, not published");
    assert_eq!(
        tracker.next_deadline(),
        Some(t + Duration::from_millis(100))
    );
    assert!(tracker.tick(t + Duration::from_millis(100)).is_empty());
    assert!(tracker.tick(t + Duration::from_millis(200)).is_empty());
    let out = tracker.tick(t + Duration::from_millis(300));
    assert_eq!(states(&out), vec![AgentState::Idle]);
    assert_eq!(
        out[0].rule.as_deref(),
        Some("default_known_agent_idle_fallback")
    );
    assert_eq!(tracker.next_deadline(), None);
}

#[test]
fn working_to_plain_idle_cap_publishes_without_rechecks() {
    let t0 = Instant::now();
    let mut tracker = working_session(t0);
    let t = t0 + Duration::from_secs(4);
    assert!(tracker
        .snapshot("s", None, snap(CLAUDE_PLAIN), t)
        .unwrap()
        .is_empty());
    let out = tracker.tick(t + Duration::from_millis(700));
    assert_eq!(states(&out), vec![AgentState::Idle]);
}

#[test]
fn pending_idle_is_cancelled_by_new_working_screen() {
    let t0 = Instant::now();
    let mut tracker = working_session(t0);
    let t = t0 + Duration::from_secs(4);
    assert!(tracker
        .snapshot("s", None, snap(CLAUDE_PLAIN), t)
        .unwrap()
        .is_empty());
    let out = tracker
        .snapshot(
            "s",
            None,
            snap(CLAUDE_WORKING),
            t + Duration::from_millis(150),
        )
        .unwrap();
    assert!(out.is_empty());
    assert_eq!(tracker.next_deadline(), None);
    assert_eq!(tracker.state("s"), Some(AgentState::Working));
}

#[test]
fn visible_idle_and_blocked_publish_immediately() {
    let t0 = Instant::now();
    let mut tracker = working_session(t0);
    let t = t0 + Duration::from_secs(4);
    let out = tracker
        .snapshot("s", None, snap(CLAUDE_IDLE_PROMPT), t)
        .unwrap();
    assert_eq!(states(&out), vec![AgentState::Idle]);
    assert_eq!(out[0].rule.as_deref(), Some("live_prompt_box"));
    let out = tracker
        .snapshot("s", None, snap(CLAUDE_BLOCKED), t + Duration::from_secs(1))
        .unwrap();
    assert_eq!(states(&out), vec![AgentState::Blocked]);
    assert_eq!(out[0].rule.as_deref(), Some("bash_permission_prompt"));
    // The same blocked screen again publishes nothing: no standing republish.
    let out = tracker
        .snapshot("s", None, snap(CLAUDE_BLOCKED), t + Duration::from_secs(2))
        .unwrap();
    assert!(out.is_empty());
}

#[test]
fn osc_fields_persist_until_replaced() {
    let t0 = Instant::now();
    let mut tracker = Tracker::new();
    tracker.start("s", Agent::Codex, t0).unwrap();
    tracker
        .snapshot(
            "s",
            None,
            SnapshotUpdate {
                screen: "x".into(),
                osc_title: Some("⠸ project".into()),
                osc_progress: None,
            },
            t0,
        )
        .unwrap();
    let out = tracker.tick(t0 + Duration::from_secs(3));
    assert_eq!(states(&out), vec![AgentState::Working]);
    // Screen changes, title absent: still working from the retained title.
    let out = tracker
        .snapshot("s", None, snap("y"), t0 + Duration::from_secs(4))
        .unwrap();
    assert!(out.is_empty());
    // Title cleared: falls to idle fallback after the hold.
    let out = tracker
        .snapshot(
            "s",
            None,
            SnapshotUpdate {
                screen: "y".into(),
                osc_title: Some(String::new()),
                osc_progress: None,
            },
            t0 + Duration::from_secs(5),
        )
        .unwrap();
    assert!(out.is_empty());
    let out = tracker.tick(t0 + Duration::from_secs(6));
    assert_eq!(states(&out), vec![AgentState::Idle]);
}

#[test]
fn transcript_viewer_holds_previous_state() {
    let t0 = Instant::now();
    let mut tracker = working_session(t0);
    let viewer = "line\nShowing detailed transcript · ctrl+o to toggle\n";
    let out = tracker
        .snapshot("s", None, snap(viewer), t0 + Duration::from_secs(4))
        .unwrap();
    assert!(out.is_empty());
    assert_eq!(tracker.state("s"), Some(AgentState::Working));
}

#[test]
fn exit_drops_session_and_timers() {
    let t0 = Instant::now();
    let mut tracker = working_session(t0);
    tracker
        .snapshot("s", None, snap(CLAUDE_PLAIN), t0 + Duration::from_secs(4))
        .unwrap();
    assert!(tracker.next_deadline().is_some());
    assert!(tracker.exit("s"));
    assert!(!tracker.exit("s"));
    assert_eq!(tracker.next_deadline(), None);
    assert!(tracker.tick(t0 + Duration::from_secs(10)).is_empty());
    assert_eq!(tracker.sessions().count(), 0);
}

#[test]
fn sessions_are_independent() {
    let t0 = Instant::now();
    let mut tracker = Tracker::new();
    tracker.start("a", Agent::Claude, t0).unwrap();
    tracker.start("b", Agent::Codex, t0).unwrap();
    tracker
        .snapshot("a", None, snap(CLAUDE_BLOCKED), t0)
        .unwrap();
    tracker
        .snapshot(
            "b",
            None,
            SnapshotUpdate {
                screen: "x".into(),
                osc_title: Some("⠸ p".into()),
                osc_progress: None,
            },
            t0,
        )
        .unwrap();
    let out = tracker.tick(t0 + Duration::from_secs(3));
    let mut pairs: Vec<(String, AgentState)> =
        out.into_iter().map(|t| (t.session, t.state)).collect();
    pairs.sort_by(|a, b| a.0.cmp(&b.0));
    assert_eq!(
        pairs,
        vec![
            ("a".into(), AgentState::Blocked),
            ("b".into(), AgentState::Working)
        ]
    );
}
