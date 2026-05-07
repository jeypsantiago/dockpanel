//! Compile-time default runbooks indexed by alert_type.
//!
//! Each entry is the source of truth for "what should this alert say if the
//! operator hasn't customized it." The const slice is consulted by:
//!
//! 1. `alert_runbooks::get_runbook` as a fallback when no DB row exists
//!    (so fresh installs still produce useful notification payloads).
//! 2. `POST /api/alerts/runbooks/apply-defaults` to seed missing rows.
//!
//! Adding a new alert type: add its `.md` file under `panel/backend/runbooks/`
//! and a new entry here. The "Apply Defaults" UX picks it up automatically.

pub struct DefaultRunbook {
    pub alert_type: &'static str,
    pub severity: &'static str,
    pub runbook_md: &'static str,
}

pub const DEFAULTS: &[DefaultRunbook] = &[
    // Critical (paging-grade) — 5 entries
    DefaultRunbook {
        alert_type: "offline",
        severity: "critical",
        runbook_md: include_str!("../../runbooks/offline.md"),
    },
    DefaultRunbook {
        alert_type: "service_down",
        severity: "critical",
        runbook_md: include_str!("../../runbooks/service_down.md"),
    },
    DefaultRunbook {
        alert_type: "container_crashloop",
        severity: "critical",
        runbook_md: include_str!("../../runbooks/container_crashloop.md"),
    },
    DefaultRunbook {
        alert_type: "backup_failure",
        severity: "critical",
        runbook_md: include_str!("../../runbooks/backup_failure.md"),
    },
    DefaultRunbook {
        alert_type: "gpu_temperature",
        severity: "critical",
        runbook_md: include_str!("../../runbooks/gpu_temperature.md"),
    },
    // Warning (notify-worthy) — 9 entries
    DefaultRunbook {
        alert_type: "cpu",
        severity: "warning",
        runbook_md: include_str!("../../runbooks/cpu.md"),
    },
    DefaultRunbook {
        alert_type: "memory",
        severity: "warning",
        runbook_md: include_str!("../../runbooks/memory.md"),
    },
    DefaultRunbook {
        alert_type: "disk",
        severity: "warning",
        runbook_md: include_str!("../../runbooks/disk.md"),
    },
    DefaultRunbook {
        alert_type: "disk_forecast",
        severity: "warning",
        runbook_md: include_str!("../../runbooks/disk_forecast.md"),
    },
    DefaultRunbook {
        alert_type: "ssl_expiry",
        severity: "warning",
        runbook_md: include_str!("../../runbooks/ssl_expiry.md"),
    },
    DefaultRunbook {
        alert_type: "container_unhealthy",
        severity: "warning",
        runbook_md: include_str!("../../runbooks/container_unhealthy.md"),
    },
    DefaultRunbook {
        alert_type: "gpu_utilization",
        severity: "warning",
        runbook_md: include_str!("../../runbooks/gpu_utilization.md"),
    },
    DefaultRunbook {
        alert_type: "gpu_vram",
        severity: "warning",
        runbook_md: include_str!("../../runbooks/gpu_vram.md"),
    },
    DefaultRunbook {
        alert_type: "memory_leak",
        severity: "warning",
        runbook_md: include_str!("../../runbooks/memory_leak.md"),
    },
    // Info — 1 entry
    DefaultRunbook {
        alert_type: "container_down",
        severity: "info",
        runbook_md: include_str!("../../runbooks/container_down.md"),
    },
];

pub fn find_default(alert_type: &str) -> Option<&'static DefaultRunbook> {
    DEFAULTS.iter().find(|d| d.alert_type == alert_type)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    #[test]
    fn fifteen_runbooks_loaded() {
        assert_eq!(DEFAULTS.len(), 15, "expected 15 default runbooks");
    }

    #[test]
    fn alert_types_are_unique() {
        let mut seen = HashSet::new();
        for d in DEFAULTS {
            assert!(
                seen.insert(d.alert_type),
                "duplicate alert_type: {}",
                d.alert_type
            );
        }
    }

    #[test]
    fn every_severity_is_valid() {
        for d in DEFAULTS {
            assert!(
                matches!(d.severity, "info" | "warning" | "critical"),
                "invalid severity '{}' on {}",
                d.severity,
                d.alert_type
            );
        }
    }

    #[test]
    fn every_runbook_has_content() {
        for d in DEFAULTS {
            assert!(
                d.runbook_md.len() > 100,
                "{} runbook is suspiciously short ({} chars)",
                d.alert_type,
                d.runbook_md.len()
            );
            assert!(
                d.runbook_md.contains("First check"),
                "{} runbook missing 'First check' section",
                d.alert_type
            );
            assert!(
                d.runbook_md.contains("Common causes"),
                "{} runbook missing 'Common causes' section",
                d.alert_type
            );
            assert!(
                d.runbook_md.contains("Escalation"),
                "{} runbook missing 'Escalation' section",
                d.alert_type
            );
        }
    }

    #[test]
    fn severity_ladder_matches_design() {
        let critical: HashSet<&str> = DEFAULTS
            .iter()
            .filter(|d| d.severity == "critical")
            .map(|d| d.alert_type)
            .collect();
        let expected_critical: HashSet<&str> = [
            "offline",
            "service_down",
            "container_crashloop",
            "backup_failure",
            "gpu_temperature",
        ]
        .iter()
        .copied()
        .collect();
        assert_eq!(
            critical, expected_critical,
            "critical severity set drifted from design §2"
        );
    }

    #[test]
    fn find_default_works_for_known_and_unknown() {
        assert!(find_default("offline").is_some());
        assert!(find_default("does_not_exist").is_none());
    }
}

