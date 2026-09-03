//! Shim for herdr's `crate::detect`. Provides the agent enum, label parsing,
//! and the detection result types that herdr's `manifest.rs` imports from its
//! parent module, then includes that file by path. Process identification,
//! which is the rest of herdr's `detect/mod.rs`, is deliberately absent: the
//! caller already knows which harness runs in a session.

// herdr's manifest engine: rule schema, validation, regions, gates,
// bundled and override manifest loading, and explain output.
#[rustfmt::skip]
#[allow(dead_code)]
#[path = "../../../../src/detect/manifest.rs"]
pub mod manifest;

pub(crate) mod manifest_update;

/// The detected state of a terminal session. Mirrors herdr's enum exactly.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AgentState {
    /// Agent finished, prompt visible, nothing happening.
    Idle,
    /// Agent is actively working/processing.
    Working,
    /// Agent needs human input and is blocked on a response.
    Blocked,
    /// Not yet classified, or an agent-owned viewer that hides live state.
    Unknown,
}

/// Screen-derived agent state plus the visible-evidence flags herdr uses for
/// source arbitration and publish stabilization.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AgentDetection {
    pub state: AgentState,
    /// True when the screen is an agent-owned viewer (transcript, picker)
    /// whose content must not update state.
    pub skip_state_update: bool,
    /// True when live idle chrome is visible.
    pub visible_idle: bool,
    /// True when live UI chrome asking for human input is visible.
    pub visible_blocker: bool,
    /// True when live working chrome is visible.
    pub visible_working: bool,
}

/// The harnesses this crate classifies.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Agent {
    Claude,
    Codex,
}

impl Agent {
    pub const ALL: [Self; 2] = [Self::Claude, Self::Codex];

    /// Agents whose state comes from a screen manifest. Read by herdr's
    /// manifest cache to decide which bundled manifests to load.
    pub const SCREEN_MANIFEST_AGENTS: [Self; 2] = [Self::Claude, Self::Codex];
}

/// The manifest id and canonical label for an agent.
pub fn agent_label(agent: Agent) -> &'static str {
    match agent {
        Agent::Claude => "claude",
        Agent::Codex => "codex",
    }
}

/// Parse a label or alias. Accepts the names herdr accepts for these two
/// agents, case-insensitively, with any path prefix removed.
pub fn parse_agent_label(label: &str) -> Option<Agent> {
    let name = label.trim().to_lowercase();
    let name = name.rsplit(['/', '\\']).next().unwrap_or(&name);
    match name {
        "claude" | "claude-code" => Some(Agent::Claude),
        "codex" => Some(Agent::Codex),
        _ => None,
    }
}

/// Detect state using screen content plus OSC title and progress strings.
/// Mirrors herdr's function of the same name; `None` yields `Unknown`.
pub fn detect_agent_with_osc(
    agent: Option<Agent>,
    screen_content: &str,
    osc_title: &str,
    osc_progress: &str,
) -> AgentDetection {
    let Some(agent) = agent else {
        return AgentDetection {
            state: AgentState::Unknown,
            skip_state_update: false,
            visible_idle: false,
            visible_blocker: false,
            visible_working: false,
        };
    };
    manifest::detect_with_osc(
        agent,
        manifest::DetectionInput {
            screen: screen_content,
            osc_title,
            osc_progress,
        },
    )
}
