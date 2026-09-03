//! Shim for herdr's `crate::config`. Only `config_dir` is reached by the
//! included detection source; it locates local manifest overrides at
//! `<config_dir>/agent-detection/<agent>.toml`.

use std::path::PathBuf;

/// `$AGENTSTATE_CONFIG_DIR`, else `$XDG_CONFIG_HOME/agentstate`, else
/// `~/.config/agentstate`. Deliberately not herdr's own directory, so a
/// herdr user's overrides never leak into another environment's verdicts.
pub fn config_dir() -> PathBuf {
    if let Some(dir) = std::env::var_os("AGENTSTATE_CONFIG_DIR") {
        return PathBuf::from(dir);
    }
    if let Some(xdg) = std::env::var_os("XDG_CONFIG_HOME") {
        return PathBuf::from(xdg).join("agentstate");
    }
    let home = std::env::var_os("HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("."));
    home.join(".config").join("agentstate")
}
