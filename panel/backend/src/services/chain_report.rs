// Chain-of-trust report rendering: lazy-install typst CLI on first use, then
// transform a ChainReport JSON into a PDF via the bundled .typ template.
//
// Phase 4 W1.3 (v2.8.1). Site-only for now; v2.8.2 extends to db+volume once
// those tables get sha256_hash columns.

use std::path::{Path, PathBuf};
use tokio::process::Command;
use tokio::sync::Mutex;

const TYPST_VERSION: &str = "0.13.0";
const TYPST_DIR: &str = "/var/lib/dockpanel/typst";
const TYPST_BIN: &str = "/var/lib/dockpanel/typst/typst";
const INSTALL_TIMEOUT_SECS: u64 = 90;
const COMPILE_TIMEOUT_SECS: u64 = 30;

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

    let arch = match std::env::consts::ARCH {
        "x86_64" => "x86_64",
        "aarch64" => "aarch64",
        other => return Err(format!("unsupported arch for typst: {other}")),
    };

    let target = format!("{arch}-unknown-linux-musl");
    let url = format!(
        "https://github.com/typst/typst/releases/download/v{TYPST_VERSION}/typst-{target}.tar.xz"
    );

    // Stream the tarball through `tar -xJ` straight into the install dir.
    // No tempfile, no sha256 (matches the existing grype installer pattern;
    // TLS + github.com is the trust anchor). v2.8.2 should add checksum pinning.
    let cmd = format!(
        "set -euo pipefail; \
         curl -sSfL --max-time 60 '{url}' \
           | tar -xJf - -C '{TYPST_DIR}' --strip-components=1 'typst-{target}/typst'; \
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
