// Chain-of-trust report rendering: lazy-install typst CLI on first use, then
// transform a ChainReport JSON into a PDF via the bundled .typ template.
//
// Phase 4 W1.3 shipped site-only in v2.8.1. v2.8.2 extends to db + volume
// (the route handlers + builder are in routes/backup_orchestrator.rs) and
// adds SHA-256 pinning of the typst tarball — TLS + github.com is no longer
// the sole trust anchor.

use std::path::{Path, PathBuf};
use tokio::process::Command;
use tokio::sync::Mutex;

const TYPST_VERSION: &str = "0.13.0";
const TYPST_DIR: &str = "/var/lib/dockpanel/typst";
const TYPST_BIN: &str = "/var/lib/dockpanel/typst/typst";
const INSTALL_TIMEOUT_SECS: u64 = 120;
const COMPILE_TIMEOUT_SECS: u64 = 30;

// Pinned tarball SHA-256s for typst v0.13.0 (verified against
// github.com/typst/typst/releases/download/v0.13.0/, 2026-04-30).
// If TYPST_VERSION above is bumped, regenerate these.
const TYPST_SHA256_X86_64_MUSL: &str =
    "cd1148da61d6844e62c330fc6222e988480acafe33b76daec8eb5d221258feb6";
const TYPST_SHA256_AARCH64_MUSL: &str =
    "1a1b3841ee1d84d130c4fd58f1ac8a23acf0d6bf11161c5246f016622cf046e6";

// Embedded so the binary is self-contained; written to a tempfile per render.
const TEMPLATE_TYP: &str = include_str!("../../templates/chain-report.typ");

static INSTALL_LOCK: Mutex<()> = Mutex::const_new(());

/// Ensure the typst binary is installed on the panel host. First call fetches
/// + extracts; subsequent calls are no-ops. Returns the binary path.
pub async fn ensure_typst_installed() -> Result<PathBuf, String> {
    let bin = PathBuf::from(TYPST_BIN);
    if bin.exists() {
        return Ok(bin);
    }

    let _guard = INSTALL_LOCK.lock().await;
    // Re-check after acquiring lock — another caller may have installed.
    if bin.exists() {
        return Ok(bin);
    }

    tokio::fs::create_dir_all(TYPST_DIR)
        .await
        .map_err(|e| format!("create typst dir: {e}"))?;

    let (arch, expected_sha) = match std::env::consts::ARCH {
        "x86_64" => ("x86_64", TYPST_SHA256_X86_64_MUSL),
        "aarch64" => ("aarch64", TYPST_SHA256_AARCH64_MUSL),
        other => return Err(format!("unsupported arch for typst: {other}")),
    };

    // Allow operators to override the pinned digest if they're using a
    // different typst version (e.g. air-gapped mirror). Same env per arch.
    let env_key = format!("DOCKPANEL_TYPST_SHA256_{}", arch.to_uppercase());
    let expected_sha = std::env::var(&env_key).unwrap_or_else(|_| expected_sha.to_string());

    let target = format!("{arch}-unknown-linux-musl");
    let url = format!(
        "https://github.com/typst/typst/releases/download/v{TYPST_VERSION}/typst-{target}.tar.xz"
    );

    // Two-phase install: download to tempfile → verify sha256 → extract.
    // Adds ~30MB temp + a second pass over the bytes; runs once per host.
    // Refusing to extract a tarball whose checksum doesn't match the pin is
    // the whole point — never let `tar` see unverified bytes.
    let cmd = format!(
        "set -euo pipefail; \
         tmp=$(mktemp '{TYPST_DIR}/typst-download.XXXXXX.tar.xz'); \
         trap 'rm -f \"$tmp\"' EXIT; \
         curl -sSfL --max-time 90 -o \"$tmp\" '{url}'; \
         actual=$(sha256sum \"$tmp\" | awk '{{print $1}}'); \
         if [ \"$actual\" != '{expected_sha}' ]; then \
           echo \"typst sha256 mismatch: expected {expected_sha}, got $actual\" >&2; \
           exit 99; \
         fi; \
         tar -xJf \"$tmp\" -C '{TYPST_DIR}' --strip-components=1 'typst-{target}/typst'; \
         chmod 755 '{TYPST_BIN}'"
    );

    let output = tokio::time::timeout(
        std::time::Duration::from_secs(INSTALL_TIMEOUT_SECS),
        Command::new("bash").arg("-c").arg(&cmd).output(),
    )
    .await
    .map_err(|_| format!("typst install timed out after {INSTALL_TIMEOUT_SECS}s"))?
    .map_err(|e| format!("typst install spawn failed: {e}"))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        // Surface the sha256 mismatch separately so it doesn't get buried.
        if output.status.code() == Some(99) {
            return Err(format!("typst install aborted on sha256 mismatch: {stderr}"));
        }
        return Err(format!("typst install failed: {stderr}"));
    }

    if !bin.exists() {
        return Err("typst install completed but binary missing".to_string());
    }

    tracing::info!("typst v{TYPST_VERSION} installed at {TYPST_BIN}");
    Ok(bin)
}

/// Render a ChainReport JSON value into PDF bytes via typst. Caller is
/// responsible for ensuring `data` is a valid JSON shape consumed by
/// templates/chain-report.typ (see `ChainReport` in routes/backup_orchestrator.rs).
pub async fn render_chain_report_pdf(data: &serde_json::Value) -> Result<Vec<u8>, String> {
    let typst_bin = ensure_typst_installed().await?;

    // Use a per-render tempdir to avoid contention. /tmp is fine — files are
    // small (JSON < 100KB, PDF < 1MB) and cleaned at the end of this fn.
    let id = uuid::Uuid::new_v4();
    let dir = PathBuf::from(format!("/tmp/dockpanel-chainreport-{id}"));
    tokio::fs::create_dir_all(&dir)
        .await
        .map_err(|e| format!("tempdir create: {e}"))?;

    let result = render_inner(&typst_bin, &dir, data).await;

    // Best-effort cleanup; never block the response on it.
    let _ = tokio::fs::remove_dir_all(&dir).await;

    result
}

async fn render_inner(
    typst_bin: &Path,
    dir: &Path,
    data: &serde_json::Value,
) -> Result<Vec<u8>, String> {
    let json_path = dir.join("data.json");
    let typ_path = dir.join("chain-report.typ");
    let pdf_path = dir.join("out.pdf");

    let json_bytes = serde_json::to_vec_pretty(data)
        .map_err(|e| format!("serialize chain report: {e}"))?;
    tokio::fs::write(&json_path, &json_bytes)
        .await
        .map_err(|e| format!("write json: {e}"))?;
    tokio::fs::write(&typ_path, TEMPLATE_TYP)
        .await
        .map_err(|e| format!("write template: {e}"))?;

    // typst's --root jails file access to that dir AND treats absolute paths
    // inside the template as relative to it. Pass the JSON path as relative.
    let output = tokio::time::timeout(
        std::time::Duration::from_secs(COMPILE_TIMEOUT_SECS),
        Command::new(typst_bin)
            .arg("compile")
            .arg("--root")
            .arg(dir)
            .arg("--input")
            .arg("data_path=data.json")
            .arg(&typ_path)
            .arg(&pdf_path)
            .output(),
    )
    .await
    .map_err(|_| format!("typst compile timed out after {COMPILE_TIMEOUT_SECS}s"))?
    .map_err(|e| format!("typst compile spawn failed: {e}"))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(format!("typst compile failed: {stderr}"));
    }

    let pdf = tokio::fs::read(&pdf_path)
        .await
        .map_err(|e| format!("read generated pdf: {e}"))?;

    if pdf.len() < 4 || &pdf[..4] != b"%PDF" {
        return Err("typst produced non-PDF output".to_string());
    }

    Ok(pdf)
}
