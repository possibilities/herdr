//! `agentstate` command line.
//!
//! ```text
//! agentstate explain --agent claude|codex --screen FILE [--osc-title T] [--osc-progress P]
//! agentstate track
//! agentstate --version
//! ```
//!
//! `track` reads JSONL on stdin and writes JSONL on stdout, one line per
//! published state change across any number of sessions. stdout carries
//! nothing else; diagnostics go to stderr.

use std::io::{self, BufRead, Read as _, Write};
use std::sync::mpsc;
use std::time::{Instant, SystemTime, UNIX_EPOCH};

use agentstate::{agent_state_label, parse_agent_label, Agent, SnapshotUpdate, Tracker};
use serde::Deserialize;

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let code = match args.first().map(String::as_str) {
        Some("explain") => explain(&args[1..]),
        Some("track") => track(&args[1..]),
        Some("--version") | Some("-V") | Some("version") => {
            println!(
                "agentstate {} (herdr {})",
                env!("CARGO_PKG_VERSION"),
                agentstate::HERDR_SHA.unwrap_or("unknown")
            );
            0
        }
        Some("--help") | Some("-h") | Some("help") | None => {
            usage();
            if args.is_empty() {
                64
            } else {
                0
            }
        }
        Some(other) => {
            eprintln!("agentstate: unknown command {other}");
            usage();
            64
        }
    };
    std::process::exit(code);
}

fn usage() {
    eprintln!("usage:");
    eprintln!("  agentstate explain --agent claude|codex --screen FILE [--osc-title T] [--osc-progress P]");
    eprintln!("  agentstate track");
    eprintln!("  agentstate --version");
    eprintln!();
    eprintln!("explain: FILE is screen text (`-` for stdin); prints herdr's explain JSON.");
    eprintln!("track: JSONL in, JSONL out. Input lines:");
    eprintln!("  {{\"session\":S,\"agent\":A,\"started\":true}}");
    eprintln!("  {{\"session\":S,\"agent\":A,\"ts_ms\":N,\"screen\":TEXT,\"osc_title\":T|null,\"osc_progress\":P|null}}");
    eprintln!("  {{\"session\":S,\"exited\":true}}");
    eprintln!("Output lines: {{\"session\":S,\"state\":idle|working|blocked|unknown,\"rule\":R|null,\"ts_ms\":N}}");
}

fn explain(args: &[String]) -> i32 {
    let mut agent = None;
    let mut screen_path = None;
    let mut osc_title = String::new();
    let mut osc_progress = String::new();
    let mut iter = args.iter();
    while let Some(arg) = iter.next() {
        match arg.as_str() {
            "--agent" => agent = iter.next().cloned(),
            "--screen" => screen_path = iter.next().cloned(),
            "--osc-title" => osc_title = iter.next().cloned().unwrap_or_default(),
            "--osc-progress" => osc_progress = iter.next().cloned().unwrap_or_default(),
            "--json" => {}
            other => {
                eprintln!("agentstate explain: unknown argument {other}");
                usage();
                return 64;
            }
        }
    }
    let (Some(agent), Some(screen_path)) = (agent, screen_path) else {
        usage();
        return 64;
    };
    let Some(agent) = parse_agent_label(&agent) else {
        eprintln!("agentstate explain: unknown agent {agent}; expected claude or codex");
        return 64;
    };
    let screen = if screen_path == "-" {
        let mut text = String::new();
        if let Err(err) = io::stdin().read_to_string(&mut text) {
            eprintln!("agentstate explain: failed to read stdin: {err}");
            return 1;
        }
        text
    } else {
        match std::fs::read_to_string(&screen_path) {
            Ok(text) => text,
            Err(err) => {
                eprintln!("agentstate explain: failed to read {screen_path}: {err}");
                return 1;
            }
        }
    };
    let explain = agentstate::explain(agent, &screen, &osc_title, &osc_progress);
    println!("{}", agentstate::explain_to_json_value(&explain));
    0
}

/// Distinguishes an absent field from an explicit null.
fn some_or_null<'de, D>(deserializer: D) -> Result<Option<Option<String>>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    Option::<String>::deserialize(deserializer).map(Some)
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct InputLine {
    session: String,
    #[serde(default)]
    agent: Option<String>,
    #[serde(default)]
    started: Option<bool>,
    #[serde(default)]
    exited: Option<bool>,
    #[serde(default)]
    #[allow(dead_code)]
    ts_ms: Option<u64>,
    #[serde(default)]
    screen: Option<String>,
    #[serde(default, deserialize_with = "some_or_null")]
    osc_title: Option<Option<String>>,
    #[serde(default, deserialize_with = "some_or_null")]
    osc_progress: Option<Option<String>>,
}

fn wall_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

fn emit(out: &mut impl Write, transition: &agentstate::Transition) {
    let line = serde_json::json!({
        "session": transition.session,
        "state": agent_state_label(transition.state),
        "rule": transition.rule,
        "ts_ms": wall_ms(),
    });
    // A closed stdout means the parent is gone; there is nobody to report to.
    let _ = writeln!(out, "{line}");
    let _ = out.flush();
}

fn parse_agent_field(agent: Option<&str>) -> Result<Option<Agent>, String> {
    match agent {
        None => Ok(None),
        Some(label) => parse_agent_label(label)
            .map(Some)
            .ok_or_else(|| format!("unknown agent {label:?}; expected claude or codex")),
    }
}

fn handle_line(
    tracker: &mut Tracker,
    line: &str,
    now: Instant,
) -> Result<Vec<agentstate::Transition>, String> {
    let input: InputLine =
        serde_json::from_str(line).map_err(|err| format!("invalid JSON: {err}"))?;
    if input.session.is_empty() {
        return Err("session must not be empty".to_string());
    }
    if input.exited == Some(true) {
        if !tracker.exit(&input.session) {
            return Err(format!("session {} was not tracked", input.session));
        }
        return Ok(Vec::new());
    }
    let agent = parse_agent_field(input.agent.as_deref())?;
    if input.started == Some(true) {
        let Some(agent) = agent else {
            return Err("started requires agent".to_string());
        };
        return tracker
            .start(&input.session, agent, now)
            .map_err(|err| err.to_string());
    }
    let Some(screen) = input.screen else {
        return Err("line has no screen, started, or exited".to_string());
    };
    let update = SnapshotUpdate {
        screen,
        osc_title: input.osc_title.map(Option::unwrap_or_default),
        osc_progress: input.osc_progress.map(Option::unwrap_or_default),
    };
    tracker
        .snapshot(&input.session, agent, update, now)
        .map_err(|err| err.to_string())
}

fn track(args: &[String]) -> i32 {
    if !args.is_empty() {
        eprintln!("agentstate track: takes no arguments");
        usage();
        return 64;
    }
    let (tx, rx) = mpsc::channel::<String>();
    std::thread::spawn(move || {
        let stdin = io::stdin();
        for line in stdin.lock().lines() {
            match line {
                Ok(line) => {
                    if tx.send(line).is_err() {
                        break;
                    }
                }
                Err(err) => {
                    eprintln!("agentstate track: stdin read error: {err}");
                    break;
                }
            }
        }
    });

    let mut tracker = Tracker::new();
    let stdout = io::stdout();
    let mut out = stdout.lock();
    loop {
        let now = Instant::now();
        let received = match tracker.next_deadline() {
            Some(deadline) => rx.recv_timeout(deadline.saturating_duration_since(now)),
            None => rx.recv().map_err(|_| mpsc::RecvTimeoutError::Disconnected),
        };
        match received {
            Ok(line) => {
                if line.trim().is_empty() {
                    continue;
                }
                match handle_line(&mut tracker, &line, Instant::now()) {
                    Ok(transitions) => {
                        for transition in &transitions {
                            emit(&mut out, transition);
                        }
                    }
                    Err(err) => eprintln!("agentstate track: skipped line: {err}"),
                }
            }
            Err(mpsc::RecvTimeoutError::Timeout) => {}
            Err(mpsc::RecvTimeoutError::Disconnected) => break,
        }
        for transition in tracker.tick(Instant::now()) {
            emit(&mut out, &transition);
        }
    }
    // stdin closed: the owner is done with us. Pending holds are abandoned.
    0
}
