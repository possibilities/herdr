//! End-to-end checks of the `agentstate` binary.

use std::io::{BufRead, BufReader, Write};
use std::process::{Command, Stdio};

fn bin() -> Command {
    Command::new(env!("CARGO_BIN_EXE_agentstate"))
}

#[test]
fn version_prints_crate_version() {
    let out = bin().arg("--version").output().unwrap();
    assert!(out.status.success());
    let text = String::from_utf8(out.stdout).unwrap();
    assert!(
        text.starts_with(&format!("agentstate {} (herdr ", env!("CARGO_PKG_VERSION"))),
        "{text}"
    );
}

#[test]
fn explain_reads_a_screen_file() {
    let fixture = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/claude/idle-prompt-box.txt");
    let out = bin()
        .args(["explain", "--agent", "claude", "--screen"])
        .arg(&fixture)
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let json: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(json["state"], "idle");
    assert_eq!(json["matched_rule"]["id"], "live_prompt_box");
    assert!(json["evaluated_rules"].as_array().unwrap().len() > 5);
}

#[test]
fn track_streams_transitions_and_survives_bad_lines() {
    let mut child = bin()
        .arg("track")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    let mut stdin = child.stdin.take().unwrap();
    let stdout = child.stdout.take().unwrap();
    let mut lines = BufReader::new(stdout).lines();

    writeln!(stdin, r#"{{"session":"a","agent":"codex","started":true}}"#).unwrap();
    let first: serde_json::Value = serde_json::from_str(&lines.next().unwrap().unwrap()).unwrap();
    assert_eq!(first["session"], "a");
    assert_eq!(first["state"], "unknown");
    assert!(first["rule"].is_null());
    assert!(first["ts_ms"].as_u64().unwrap() > 0);

    writeln!(stdin, "this is not json").unwrap();
    writeln!(stdin, r#"{{"session":"a","agent":"claude","screen":"x"}}"#).unwrap();

    // Blocked screen with an "Action Required" title publishes once the 3 s
    // startup grace expires, from track's own timer.
    let screen = "• Working (4s • esc to interrupt)\n› 1. Yes, proceed\nPress enter to confirm or esc to cancel\n";
    let line = serde_json::json!({
        "session": "a", "agent": "codex", "ts_ms": 1,
        "screen": screen, "osc_title": "project", "osc_progress": null
    });
    writeln!(stdin, "{line}").unwrap();
    let second: serde_json::Value = serde_json::from_str(&lines.next().unwrap().unwrap()).unwrap();
    assert_eq!(second["state"], "blocked");
    assert_eq!(second["rule"], "live_strong_blocker");

    writeln!(stdin, r#"{{"session":"a","exited":true}}"#).unwrap();
    drop(stdin);
    let status = child.wait().unwrap();
    assert!(status.success());
    assert!(lines.next().is_none(), "nothing after exit");
    let mut stderr = String::new();
    std::io::Read::read_to_string(&mut child.stderr.take().unwrap(), &mut stderr).unwrap();
    assert!(stderr.contains("skipped line: invalid JSON"), "{stderr}");
    assert!(
        stderr.contains("is codex but the update says claude"),
        "{stderr}"
    );
}
