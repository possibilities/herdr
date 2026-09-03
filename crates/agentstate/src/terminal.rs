//! Shim for herdr's `crate::terminal`. The included stabilization policy
//! calls `terminal::state::stabilize_agent_detection`, which in herdr is the
//! identity on the raw policy state.

pub mod state {
    use crate::detect::{AgentDetection, AgentState};

    pub(crate) fn stabilize_agent_detection(detection: AgentDetection) -> AgentState {
        detection.state
    }
}
