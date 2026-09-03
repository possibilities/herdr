//! Claude Code and Codex agent state detection, extracted from herdr.
//!
//! The detection engine, the bundled manifests, and the publish-stabilization
//! policy are herdr's own source files, included by path from the enclosing
//! herdr checkout rather than copied. The modules `config`, `detect`, and
//! `terminal` are shims that provide the handful of herdr-internal symbols
//! those files reach for, so a herdr internal change surfaces here as a
//! compile error instead of silent drift.
//!
//! Inputs are three strings per snapshot: the screen text (the last `rows`
//! rows of the active screen, each row trimmed of trailing whitespace,
//! trailing blank rows dropped, rows joined by newline), the latest OSC 0/2
//! title, and the latest OSC 9 progress payload. Pass empty strings when a
//! value is unavailable.

pub mod config;
pub mod detect;
pub mod terminal;
pub mod tracker;

// herdr's pane-level stabilization policy: pending-idle confirmation, publish
// decisions, and the startup grace constant. Pure functions over explicit
// `Instant` values, so they run without a PTY.
#[rustfmt::skip]
#[allow(dead_code)]
#[path = "../../../src/pane/agent_detection.rs"]
pub(crate) mod agent_detection;

pub use detect::manifest::{
    agent_state_label, explain_to_json_value, DetectionExplain, DetectionInput, MatchedRule,
    DEFAULT_KNOWN_AGENT_IDLE_FALLBACK,
};
pub use detect::{agent_label, parse_agent_label, Agent, AgentDetection, AgentState};
pub use tracker::{SnapshotUpdate, Tracker, TrackerError, Transition};

/// Herdr commit this crate was built from, when the build set
/// `AGENTSTATE_HERDR_SHA` (the workshop installer does).
pub const HERDR_SHA: Option<&str> = option_env!("AGENTSTATE_HERDR_SHA");

/// Classify one snapshot. Equivalent to herdr's `detect_agent_with_osc`.
pub fn detect(agent: Agent, screen: &str, osc_title: &str, osc_progress: &str) -> AgentDetection {
    detect::detect_agent_with_osc(Some(agent), screen, osc_title, osc_progress)
}

/// Classify one snapshot with the full rule-evaluation evidence herdr's
/// `agent explain` prints.
pub fn explain(
    agent: Agent,
    screen: &str,
    osc_title: &str,
    osc_progress: &str,
) -> DetectionExplain {
    detect::manifest::explain_with_input(
        agent,
        DetectionInput {
            screen,
            osc_title,
            osc_progress,
        },
    )
}

/// Reload manifests from the bundled set and any local override under
/// `config::config_dir()/agent-detection/<agent>.toml`.
pub fn reload_manifests() {
    detect::manifest::reload_manifests();
}
