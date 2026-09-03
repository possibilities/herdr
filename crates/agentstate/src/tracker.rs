//! Multi-session state tracking with herdr's time behavior and an explicit
//! clock.
//!
//! herdr re-reads every pane on a 300 ms tick and re-evaluates it even when
//! the screen did not change. Detection is a pure function of the screen and
//! OSC strings, so the only tick-driven outcomes are the pending-idle
//! confirmation (a working-to-plain-idle transition is held until three
//! consecutive 100 ms rechecks agree or 700 ms have passed) and the 3 s grace
//! after a new agent process is detected. This tracker keeps the last snapshot
//! per session and runs those two timers itself, so a caller that pushes
//! snapshots only on change gets the same transitions herdr would publish.

use std::collections::HashMap;
use std::time::Instant;

use crate::agent_detection::{
    decide_screen_detection_publish, DetectionPublishDecision, PendingIdleConfirmation,
    ScreenDetectionPublishInput, AGENT_PENDING_IDLE_RECHECK, AGENT_STARTUP_GRACE_WINDOW,
};
use crate::detect::manifest::{explain_with_input, DetectionInput};
use crate::detect::{Agent, AgentDetection, AgentState};

/// One published state change.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Transition {
    pub session: String,
    pub state: AgentState,
    /// The manifest rule that decided the state, or herdr's fallback reason
    /// when no rule matched. `None` for the initial `Unknown`.
    pub rule: Option<String>,
}

/// A screen update for a session. `None` for an OSC field means unchanged
/// since the previous snapshot; pass `Some(String::new())` to clear one.
#[derive(Debug, Clone, Default)]
pub struct SnapshotUpdate {
    pub screen: String,
    pub osc_title: Option<String>,
    pub osc_progress: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TrackerError {
    /// The session exists with a different agent.
    AgentMismatch {
        session: String,
        existing: Agent,
        requested: Agent,
    },
    /// The session does not exist and the update carried no agent.
    UnknownSession(String),
}

impl std::fmt::Display for TrackerError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::AgentMismatch {
                session,
                existing,
                requested,
            } => write!(
                f,
                "session {session} is {} but the update says {}",
                crate::agent_label(*existing),
                crate::agent_label(*requested)
            ),
            Self::UnknownSession(session) => {
                write!(
                    f,
                    "session {session} is not tracked and the update names no agent"
                )
            }
        }
    }
}

impl std::error::Error for TrackerError {}

#[derive(Debug)]
struct SessionTrack {
    agent: Agent,
    state: AgentState,
    last_visible_idle: bool,
    last_visible_blocker: bool,
    last_visible_working: bool,
    last_visible_signal_refresh: Option<Instant>,
    pending_idle: PendingIdleConfirmation,
    pending_recheck_at: Option<Instant>,
    grace_until: Option<Instant>,
    screen: Option<String>,
    osc_title: String,
    osc_progress: String,
}

impl SessionTrack {
    fn new(agent: Agent, now: Instant) -> Self {
        Self {
            agent,
            state: AgentState::Unknown,
            last_visible_idle: false,
            last_visible_blocker: false,
            last_visible_working: false,
            last_visible_signal_refresh: None,
            pending_idle: PendingIdleConfirmation::default(),
            pending_recheck_at: None,
            grace_until: Some(now + AGENT_STARTUP_GRACE_WINDOW),
            screen: None,
            osc_title: String::new(),
            osc_progress: String::new(),
        }
    }

    fn deadline(&self) -> Option<Instant> {
        match (self.grace_until, self.pending_recheck_at) {
            (Some(grace), _) => Some(grace),
            (None, pending) => pending,
        }
    }

    /// Evaluate the held snapshot. Returns the new state when it changed.
    fn evaluate(&mut self, now: Instant) -> Option<(AgentState, Option<String>)> {
        let screen = self.screen.as_deref()?;
        let explain = explain_with_input(
            self.agent,
            DetectionInput {
                screen,
                osc_title: &self.osc_title,
                osc_progress: &self.osc_progress,
            },
        );
        if explain.skip_state_update {
            // herdr: an agent-owned viewer holds the previous state.
            self.pending_idle.clear();
            self.pending_recheck_at = None;
            return None;
        }
        let detection = AgentDetection {
            state: explain.state,
            skip_state_update: false,
            visible_idle: explain.visible_idle,
            visible_blocker: explain.visible_blocker,
            visible_working: explain.visible_working,
        };
        let decision = decide_screen_detection_publish(
            ScreenDetectionPublishInput {
                current_state: self.state,
                last_visible_idle: self.last_visible_idle,
                last_visible_blocker: self.last_visible_blocker,
                last_visible_working: self.last_visible_working,
                last_visible_signal_refresh: self.last_visible_signal_refresh,
                screen_detection: detection,
                process_exited: false,
                agent_changed: false,
                now,
            },
            &mut self.pending_idle,
        );
        self.pending_recheck_at = self
            .pending_idle
            .active()
            .then(|| now + AGENT_PENDING_IDLE_RECHECK);
        match decision {
            DetectionPublishDecision::NoPublish => None,
            DetectionPublishDecision::Publish {
                state,
                visible_idle,
                visible_blocker,
                visible_working,
                ..
            } => {
                let changed = state != self.state;
                self.state = state;
                self.last_visible_idle = visible_idle;
                self.last_visible_blocker = visible_blocker;
                self.last_visible_working = visible_working;
                self.last_visible_signal_refresh =
                    (visible_blocker || visible_working).then_some(now);
                changed.then(|| {
                    let rule = explain
                        .matched_rule
                        .map(|rule| rule.id)
                        .or(explain.fallback_reason);
                    (state, rule)
                })
            }
        }
    }
}

/// Tracks many sessions. All timing is driven by the `Instant` values the
/// caller passes; nothing here reads the clock.
#[derive(Debug, Default)]
pub struct Tracker {
    sessions: HashMap<String, SessionTrack>,
}

impl Tracker {
    pub fn new() -> Self {
        Self::default()
    }

    /// Begin tracking a session before its first screen arrives. Starts the
    /// 3 s startup grace and publishes `Unknown`. Idempotent for an existing
    /// session with the same agent.
    pub fn start(
        &mut self,
        session: &str,
        agent: Agent,
        now: Instant,
    ) -> Result<Vec<Transition>, TrackerError> {
        if let Some(existing) = self.sessions.get(session) {
            if existing.agent != agent {
                return Err(TrackerError::AgentMismatch {
                    session: session.to_string(),
                    existing: existing.agent,
                    requested: agent,
                });
            }
            return Ok(Vec::new());
        }
        self.sessions
            .insert(session.to_string(), SessionTrack::new(agent, now));
        Ok(vec![Transition {
            session: session.to_string(),
            state: AgentState::Unknown,
            rule: None,
        }])
    }

    /// Record a new screen for a session, creating the session when `agent`
    /// is given and it is new. Returns any transitions published now; a
    /// pending working-to-idle hold publishes later from [`Tracker::tick`].
    pub fn snapshot(
        &mut self,
        session: &str,
        agent: Option<Agent>,
        update: SnapshotUpdate,
        now: Instant,
    ) -> Result<Vec<Transition>, TrackerError> {
        let mut out = Vec::new();
        if !self.sessions.contains_key(session) {
            let Some(agent) = agent else {
                return Err(TrackerError::UnknownSession(session.to_string()));
            };
            out.extend(self.start(session, agent, now)?);
        }
        let track = self
            .sessions
            .get_mut(session)
            .expect("session inserted above");
        if let Some(agent) = agent {
            if agent != track.agent {
                return Err(TrackerError::AgentMismatch {
                    session: session.to_string(),
                    existing: track.agent,
                    requested: agent,
                });
            }
        }
        track.screen = Some(update.screen);
        if let Some(title) = update.osc_title {
            track.osc_title = title;
        }
        if let Some(progress) = update.osc_progress {
            track.osc_progress = progress;
        }
        if track.grace_until.is_some_and(|until| now < until) {
            // herdr skips evaluation during startup grace; the held screen is
            // classified when the grace expires.
            track.pending_idle.clear();
            track.pending_recheck_at = None;
            return Ok(out);
        }
        track.grace_until = None;
        if let Some((state, rule)) = track.evaluate(now) {
            out.push(Transition {
                session: session.to_string(),
                state,
                rule,
            });
        }
        Ok(out)
    }

    /// Drop a session and its timers. Publishes nothing.
    pub fn exit(&mut self, session: &str) -> bool {
        self.sessions.remove(session).is_some()
    }

    /// Run every timer that is due. Call whenever [`Tracker::next_deadline`]
    /// has passed.
    pub fn tick(&mut self, now: Instant) -> Vec<Transition> {
        let mut out = Vec::new();
        let mut due: Vec<String> = self
            .sessions
            .iter()
            .filter(|(_, track)| track.deadline().is_some_and(|deadline| deadline <= now))
            .map(|(session, _)| session.clone())
            .collect();
        due.sort();
        for session in due {
            let Some(track) = self.sessions.get_mut(&session) else {
                continue;
            };
            if track.grace_until.is_some() {
                track.grace_until = None;
                track.pending_idle.clear();
            }
            track.pending_recheck_at = None;
            if let Some((state, rule)) = track.evaluate(now) {
                out.push(Transition {
                    session,
                    state,
                    rule,
                });
            }
        }
        out
    }

    /// The earliest instant at which [`Tracker::tick`] has work to do.
    pub fn next_deadline(&self) -> Option<Instant> {
        self.sessions
            .values()
            .filter_map(SessionTrack::deadline)
            .min()
    }

    pub fn state(&self, session: &str) -> Option<AgentState> {
        self.sessions.get(session).map(|track| track.state)
    }

    pub fn agent(&self, session: &str) -> Option<Agent> {
        self.sessions.get(session).map(|track| track.agent)
    }

    pub fn sessions(&self) -> impl Iterator<Item = &str> {
        self.sessions.keys().map(String::as_str)
    }
}
