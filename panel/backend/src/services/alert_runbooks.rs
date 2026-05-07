//! Lookup helper for alert runbooks.
//!
//! Resolution order: DB row first (operator-edited or seeded), then fall back
//! to the compile-time const slice. This keeps fresh installs producing
//! useful notification payloads even before the operator clicks
//! "Seed missing default runbooks."
//!
//! Notification payloads use `excerpt(280)` to keep mobile previews readable
//! across slack/discord/pagerduty. The full markdown is rendered in email
//! and in the in-panel incident detail view.

use serde::Serialize;
use sqlx::PgPool;

use crate::services::alert_runbook_defaults::{find_default, DEFAULTS};

#[derive(Debug, Clone, Serialize)]
pub struct RunbookView {
    pub alert_type: String,
    pub runbook_md: String,
    pub severity_default: String,
    pub is_default: bool,
    pub updated_at: Option<chrono::DateTime<chrono::Utc>>,
    pub updated_by: Option<uuid::Uuid>,
}

/// Resolve a runbook by alert_type. Returns DB row if present, otherwise
/// the compile-time default, otherwise None for unknown alert types.
pub async fn get_runbook(pool: &PgPool, alert_type: &str) -> Option<RunbookView> {
    if let Ok(Some((md, sev, updated_at, updated_by))) = sqlx::query_as::<
        _,
        (
            String,
            String,
            chrono::DateTime<chrono::Utc>,
            Option<uuid::Uuid>,
        ),
    >(
        "SELECT runbook_md, severity_default, updated_at, updated_by \
         FROM alert_runbooks WHERE alert_type = $1",
    )
    .bind(alert_type)
    .fetch_optional(pool)
    .await
    {
        return Some(RunbookView {
            alert_type: alert_type.to_string(),
            runbook_md: md,
            severity_default: sev,
            is_default: false,
            updated_at: Some(updated_at),
            updated_by,
        });
    }

    find_default(alert_type).map(|d| RunbookView {
        alert_type: d.alert_type.to_string(),
        runbook_md: d.runbook_md.to_string(),
        severity_default: d.severity.to_string(),
        is_default: true,
        updated_at: None,
        updated_by: None,
    })
}

/// List every known runbook — DB rows merged over the const slice. DB rows
/// win on collision; const-slice entries that have no DB row are returned
/// with `is_default = true`.
pub async fn list_runbooks(pool: &PgPool) -> Vec<RunbookView> {
    let mut out: Vec<RunbookView> = Vec::with_capacity(DEFAULTS.len() + 4);

    let rows: Vec<(
        String,
        String,
        String,
        chrono::DateTime<chrono::Utc>,
        Option<uuid::Uuid>,
    )> = sqlx::query_as(
        "SELECT alert_type, runbook_md, severity_default, updated_at, updated_by \
         FROM alert_runbooks",
    )
    .fetch_all(pool)
    .await
    .unwrap_or_default();

    for (alert_type, md, sev, updated_at, updated_by) in rows {
        out.push(RunbookView {
            alert_type,
            runbook_md: md,
            severity_default: sev,
            is_default: false,
            updated_at: Some(updated_at),
            updated_by,
        });
    }

    for d in DEFAULTS {
        if !out.iter().any(|r| r.alert_type == d.alert_type) {
            out.push(RunbookView {
                alert_type: d.alert_type.to_string(),
                runbook_md: d.runbook_md.to_string(),
                severity_default: d.severity.to_string(),
                is_default: true,
                updated_at: None,
                updated_by: None,
            });
        }
    }

    out.sort_by(|a, b| a.alert_type.cmp(&b.alert_type));
    out
}

/// Truncate markdown to ~280 chars at a sentence boundary for use in
/// notification payloads (slack/discord/pagerduty mobile preview).
/// Strips leading `#` headings so the excerpt starts with content prose.
pub fn excerpt(md: &str, max: usize) -> String {
    let stripped: String = md
        .lines()
        .filter(|l| !l.trim_start().starts_with('#'))
        .collect::<Vec<_>>()
        .join(" ");
    let trimmed = stripped.trim();

    if trimmed.chars().count() <= max {
        return trimmed.to_string();
    }

    let head: String = trimmed.chars().take(max).collect();
    if let Some(idx) = head.rfind(". ") {
        let mut out = head[..=idx].trim_end().to_string();
        out.push('…');
        return out;
    }
    if let Some(idx) = head.rfind(' ') {
        let mut out = head[..idx].to_string();
        out.push('…');
        return out;
    }
    let mut out = head;
    out.push('…');
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn excerpt_short_string_returns_unchanged() {
        let s = "Short blurb.";
        assert_eq!(excerpt(s, 280), "Short blurb.");
    }

    #[test]
    fn excerpt_truncates_at_sentence_boundary() {
        let s = "First sentence. Second sentence is much longer and should be cut off because we want a short preview.";
        let out = excerpt(s, 50);
        assert!(out.ends_with('…'));
        assert!(out.starts_with("First sentence."));
    }

    #[test]
    fn excerpt_strips_markdown_headings() {
        let md = "# Title\n\nProse line one. Prose line two.";
        let out = excerpt(md, 280);
        assert!(!out.contains('#'));
        assert!(out.contains("Prose line one"));
    }
}
