use crate::safe_cmd::safe_command;

/// Result of an end-to-end backup drill (extract → scratch container → HTTP probe → teardown).
/// Distinct from `backup_verify::VerificationResult`: a drill *runs* the restored backup,
/// it doesn't just validate the archive.
#[derive(serde::Serialize)]
pub struct DrillResult {
    pub passed: bool,
    pub http_status: Option<i32>,
    pub body_excerpt: Option<String>,
    pub error_message: Option<String>,
    pub duration_ms: u64,
}

fn drill_failure(start: std::time::Instant, msg: impl Into<String>) -> DrillResult {
    DrillResult {
        passed: false,
        http_status: None,
        body_excerpt: None,
        error_message: Some(msg.into()),
        duration_ms: start.elapsed().as_millis() as u64,
    }
}

/// Site drill: extract the backup tar to a scratch dir, mount it read-only into a fresh
/// `nginx:alpine` container with `--network none`, probe via `docker exec wget`, tear everything down.
///
/// Probe success criteria: nginx returns *any* HTTP response (even 403/404 means
/// the container booted and the mount worked). Total HTTP failure (no response,
/// connection refused, exec error) is the failure signal.
pub async fn drill_site_backup(domain: &str, filename: &str) -> Result<DrillResult, String> {
    let start = std::time::Instant::now();

    // Validation mirrors backup_verify::verify_site_backup.
    if filename.is_empty() || filename.contains("..") || filename.contains('/') {
        return Err("Invalid filename".to_string());
    }

    let backup_path = format!("/var/backups/dockpanel/{domain}/{filename}");
    if !std::path::Path::new(&backup_path).exists() {
        return Err("Backup file not found".to_string());
    }

    let drill_id = uuid::Uuid::new_v4().to_string();
    let scratch_dir = format!("/var/lib/dockpanel/drills/{drill_id}");
    let container_name = format!("dockpanel-drill-{}", &drill_id[..8]);

    // Always tear down on exit. Use a guard pattern via an inner async block.
    let result = run_site_drill(&backup_path, &scratch_dir, &container_name, start).await;

    // Cleanup container (best-effort — `--rm` should already handle it but make sure).
    let _ = safe_command("docker")
        .args(["rm", "-f", &container_name])
        .output()
        .await;

    // Cleanup scratch dir (best-effort).
    let _ = std::fs::remove_dir_all(&scratch_dir);

    Ok(result)
}

async fn run_site_drill(
    backup_path: &str,
    scratch_dir: &str,
    container_name: &str,
    start: std::time::Instant,
) -> DrillResult {
    // 1. Create scratch dir.
    if let Err(e) = std::fs::create_dir_all(scratch_dir) {
        return drill_failure(start, format!("scratch dir: {e}"));
    }

    // 2. Extract tar with timeout.
    let extract = tokio::time::timeout(
        std::time::Duration::from_secs(120),
        safe_command("tar")
            .args(["xzf", backup_path, "-C", scratch_dir, "--no-same-owner", "--no-same-permissions"])
            .output(),
    )
    .await;

    let extract_ok = extract
        .map(|r| r.map(|o| o.status.success()).unwrap_or(false))
        .unwrap_or(false);
    if !extract_ok {
        return drill_failure(start, "tar extract failed");
    }

    // 3. Spin nginx:alpine on the scratch dir, read-only mount, no network.
    //    --network none is intentional: a malicious backup can't phone home.
    //    Loopback inside the container still works so wget localhost still does.
    let run = tokio::time::timeout(
        std::time::Duration::from_secs(30),
        safe_command("docker")
            .args([
                "run", "--rm", "-d",
                "--name", container_name,
                "--network", "none",
                "--memory=128m",
                "--cpus=0.5",
                "--read-only",
                "--tmpfs", "/var/cache/nginx",
                "--tmpfs", "/var/run",
                "-v", &format!("{scratch_dir}:/usr/share/nginx/html:ro"),
                "nginx:alpine",
            ])
            .output(),
    )
    .await;

    let started = run
        .map(|r| r.map(|o| o.status.success()).unwrap_or(false))
        .unwrap_or(false);
    if !started {
        return drill_failure(start, "nginx scratch container failed to start");
    }

    // 4. Wait briefly for nginx to bind. Alpine nginx is fast (~200ms on a warm node).
    tokio::time::sleep(std::time::Duration::from_millis(500)).await;

    // 5. Probe via `docker exec wget`. nginx:alpine ships busybox wget.
    //    --server-response prints the status line to stderr; -O - emits body to stdout.
    //    -T 5 caps probe at 5s.
    let probe = tokio::time::timeout(
        std::time::Duration::from_secs(15),
        safe_command("docker")
            .args([
                "exec", container_name,
                "wget", "-q", "-O", "-", "--server-response", "-T", "5",
                "http://localhost/",
            ])
            .output(),
    )
    .await;

    match probe {
        Ok(Ok(out)) => {
            let stderr = String::from_utf8_lossy(&out.stderr);
            let stdout = String::from_utf8_lossy(&out.stdout);
            let http_status = parse_http_status(&stderr);
            let body_excerpt = if stdout.is_empty() { None } else {
                Some(stdout.chars().take(500).collect::<String>())
            };

            // wget exit 0 = 2xx response, exit 8 = server returned non-2xx.
            // Both mean "nginx is alive and serving". Exit 4 = network failure, that's a fail.
            let passed = http_status.is_some();

            DrillResult {
                passed,
                http_status,
                body_excerpt,
                error_message: if passed { None } else {
                    Some(format!("probe got no HTTP response (wget stderr: {})", stderr.chars().take(200).collect::<String>()))
                },
                duration_ms: start.elapsed().as_millis() as u64,
            }
        }
        Ok(Err(e)) => drill_failure(start, format!("docker exec failed: {e}")),
        Err(_) => drill_failure(start, "probe timeout"),
    }
}

/// Parse HTTP status from busybox wget --server-response stderr. Looks for
/// the first line matching `  HTTP/1.x NNN`.
fn parse_http_status(stderr: &str) -> Option<i32> {
    for line in stderr.lines() {
        let trimmed = line.trim_start();
        if let Some(rest) = trimmed.strip_prefix("HTTP/") {
            // rest: "1.0 200 OK" or "1.1 404 Not Found"
            let mut parts = rest.split_whitespace();
            let _ver = parts.next()?;
            let code = parts.next()?.parse::<i32>().ok()?;
            return Some(code);
        }
    }
    None
}
