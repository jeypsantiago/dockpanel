//! Safe command execution helpers.
//!
//! Every child process spawned by the agent MUST use these helpers instead of
//! raw `Command::new()`.  They call `.env_clear()` and set a minimal, safe
//! environment so that inherited variables like `LD_PRELOAD`, `LD_LIBRARY_PATH`,
//! or a tampered `PATH` cannot be used to hijack child processes.

/// Minimal safe PATH containing only system directories.
const SAFE_PATH: &str = "/usr/local/sbin:/usr/local/bin:/usr/sbin:/usr/bin:/sbin:/bin";
/// Dedicated writable home for sandboxed child commands.
const SAFE_HOME: &str = "/var/lib/dockpanel";
/// Docker CLI config directory inside the writable home.
const SAFE_DOCKER_CONFIG: &str = "/var/lib/dockpanel/docker";

/// Create an async `tokio::process::Command` with a sanitized environment.
///
/// The child process starts with an **empty** environment and only receives:
/// - `PATH`  – system directories only
/// - `HOME`  – `/var/lib/dockpanel`
/// - `DOCKER_CONFIG` – `/var/lib/dockpanel/docker`
/// - `LANG`  – `C.UTF-8`
/// - `LC_ALL` – `C.UTF-8`
///
/// Callers that need additional env vars (e.g. `PGPASSWORD`) should add them
/// via `.env("KEY", "value")` **after** calling this function.
pub fn safe_command(binary: &str) -> tokio::process::Command {
    let mut cmd = tokio::process::Command::new(binary);
    cmd.env_clear();
    cmd.env("PATH", SAFE_PATH);
    cmd.env("HOME", SAFE_HOME);
    cmd.env("DOCKER_CONFIG", SAFE_DOCKER_CONFIG);
    cmd.env("LANG", "C.UTF-8");
    cmd.env("LC_ALL", "C.UTF-8");
    cmd
}

/// Create a synchronous `std::process::Command` with a sanitized environment.
///
/// Same safety guarantees as [`safe_command`] but for blocking contexts
/// (e.g. `app_process.rs` which writes systemd units synchronously).
pub fn safe_command_sync(binary: &str) -> std::process::Command {
    let mut cmd = std::process::Command::new(binary);
    cmd.env_clear();
    cmd.env("PATH", SAFE_PATH);
    cmd.env("HOME", SAFE_HOME);
    cmd.env("DOCKER_CONFIG", SAFE_DOCKER_CONFIG);
    cmd.env("LANG", "C.UTF-8");
    cmd.env("LC_ALL", "C.UTF-8");
    cmd
}

/// Run a binary outside the agent's `ProtectSystem=strict` sandbox via
/// `systemd-run`. PID1 spawns the transient unit in its own mount namespace,
/// so the inner binary sees the full filesystem read-write — necessary for
/// commands like `apt-get update/install/upgrade` that must write to
/// `/var/cache/apt`, `/var/lib/apt/lists`, `/var/lib/dpkg`, and `/usr`,
/// none of which are in the agent unit's `ReadWritePaths`.
///
/// Use sparingly: every call escapes the sandbox, so reserve this for
/// commands that genuinely cannot run sandboxed (apt/dpkg/etc.). Read-only
/// commands like `apt list --upgradable` work fine under the sandbox and
/// should keep using [`safe_command`].
///
/// Env vars passed via `extra_env` are forwarded to the inner binary using
/// `--setenv=KEY=value`. The defaults (PATH, HOME, LANG, LC_ALL,
/// DEBIAN_FRONTEND) are always set so the inner binary doesn't inherit
/// PID1's wider environment. **`.env()` on the returned Command applies to
/// `systemd-run` itself, not the inner binary** — pass extra inner-binary
/// env via `extra_env`.
pub fn safe_command_unsandboxed(
    binary: &str,
    extra_env: &[(&str, &str)],
) -> tokio::process::Command {
    let mut cmd = tokio::process::Command::new("systemd-run");
    cmd.env_clear();
    cmd.env("PATH", SAFE_PATH);
    cmd.args(["--quiet", "--pipe", "--wait", "--collect"]);
    cmd.arg(format!("--setenv=PATH={SAFE_PATH}"));
    cmd.arg("--setenv=HOME=/root");
    cmd.arg("--setenv=LANG=C.UTF-8");
    cmd.arg("--setenv=LC_ALL=C.UTF-8");
    cmd.arg("--setenv=DEBIAN_FRONTEND=noninteractive");
    for (k, v) in extra_env {
        cmd.arg(format!("--setenv={k}={v}"));
    }
    cmd.arg("--");
    cmd.arg(binary);
    cmd
}

/// Synchronous sibling of [`safe_command_unsandboxed`] for blocking contexts
/// (e.g. `services/smtp.rs::ensure_msmtp` which installs msmtp via apt).
pub fn safe_command_sync_unsandboxed(
    binary: &str,
    extra_env: &[(&str, &str)],
) -> std::process::Command {
    let mut cmd = std::process::Command::new("systemd-run");
    cmd.env_clear();
    cmd.env("PATH", SAFE_PATH);
    cmd.args(["--quiet", "--pipe", "--wait", "--collect"]);
    cmd.arg(format!("--setenv=PATH={SAFE_PATH}"));
    cmd.arg("--setenv=HOME=/root");
    cmd.arg("--setenv=LANG=C.UTF-8");
    cmd.arg("--setenv=LC_ALL=C.UTF-8");
    cmd.arg("--setenv=DEBIAN_FRONTEND=noninteractive");
    for (k, v) in extra_env {
        cmd.arg(format!("--setenv={k}={v}"));
    }
    cmd.arg("--");
    cmd.arg(binary);
    cmd
}
