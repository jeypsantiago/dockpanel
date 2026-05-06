import { useState, useEffect, useRef } from "react";
import { api } from "../api";
import { logger } from "../utils/logger";
import { renderMarkdown } from "../utils/markdown";

interface Alert {
  id: string;
  server_id: string | null;
  site_id: string | null;
  alert_type: string;
  severity: string;
  title: string;
  message: string;
  status: string;
  notified_at: string;
  resolved_at: string | null;
  acknowledged_at: string | null;
  created_at: string;
}

interface AlertSummary {
  firing: number;
  acknowledged: number;
  resolved: number;
}

interface Runbook {
  alert_type: string;
  runbook_md: string;
  severity_default: string;
  is_default: boolean;
  updated_at: string | null;
  updated_by: string | null;
}

const TYPE_LABELS: Record<string, string> = {
  cpu: "CPU",
  memory: "Memory",
  disk: "Disk",
  disk_forecast: "Disk forecast",
  offline: "Offline",
  backup_failure: "Backup",
  ssl_expiry: "SSL",
  service_down: "Service",
  container_down: "Container down",
  container_crashloop: "Container crashloop",
  container_unhealthy: "Container unhealthy",
  gpu_utilization: "GPU utilization",
  gpu_temperature: "GPU temperature",
  gpu_vram: "GPU VRAM",
  memory_leak: "Memory leak",
  flapping: "Flapping",
};

const SEVERITY_STYLES: Record<string, { bg: string; text: string; dot: string }> = {
  critical: { bg: "bg-danger-500/10", text: "text-danger-400", dot: "bg-danger-500" },
  warning: { bg: "bg-warn-500/10", text: "text-warn-400", dot: "bg-warn-500" },
  info: { bg: "bg-accent-500/10", text: "text-accent-400", dot: "bg-accent-500" },
};

type TabId = "alerts" | "runbooks";

export default function Alerts() {
  const [tab, setTab] = useState<TabId>("alerts");
  const [alerts, setAlerts] = useState<Alert[]>([]);
  const [summary, setSummary] = useState<AlertSummary>({ firing: 0, acknowledged: 0, resolved: 0 });
  const [loading, setLoading] = useState(true);
  const [statusFilter, setStatusFilter] = useState("firing");
  const [typeFilter, setTypeFilter] = useState("");
  const [message, setMessage] = useState<{text: string; type: string} | null>(null);
  const [expanded, setExpanded] = useState<string | null>(null);
  const [runbookCache, setRunbookCache] = useState<Record<string, Runbook | null>>({});
  const refreshTimer = useRef<ReturnType<typeof setInterval>>(undefined);

  const toggleExpand = async (alert: Alert) => {
    if (expanded === alert.id) {
      setExpanded(null);
      return;
    }
    setExpanded(alert.id);
    if (runbookCache[alert.alert_type] === undefined) {
      try {
        const rb = await api.get<Runbook>(`/alerts/runbooks/${alert.alert_type}`);
        setRunbookCache((prev) => ({ ...prev, [alert.alert_type]: rb }));
      } catch {
        setRunbookCache((prev) => ({ ...prev, [alert.alert_type]: null }));
      }
    }
  };

  const fetchAlerts = async () => {
    try {
      let path = `/alerts?limit=100`;
      if (statusFilter) path += `&status=${statusFilter}`;
      if (typeFilter) path += `&alert_type=${typeFilter}`;

      const [data, sum] = await Promise.all([
        api.get<Alert[]>(path),
        api.get<AlertSummary>("/alerts/summary"),
      ]);
      setAlerts(data);
      setSummary(sum);
    } catch (e) {
      logger.error("Failed to load alerts:", e);
    } finally {
      setLoading(false);
    }
  };

  useEffect(() => {
    if (tab !== "alerts") return;
    fetchAlerts();
    refreshTimer.current = setInterval(fetchAlerts, 30000);
    return () => {
      if (refreshTimer.current) clearInterval(refreshTimer.current);
    };
  }, [statusFilter, typeFilter, tab]);

  const handleAcknowledge = async (id: string) => {
    try {
      await api.put(`/alerts/${id}/acknowledge`, {});
      setMessage({ text: "Alert acknowledged", type: "success" });
      setTimeout(() => setMessage(null), 3000);
      fetchAlerts();
    } catch (e) {
      logger.error("Failed to acknowledge alert:", e);
      setMessage({ text: "Failed to acknowledge alert", type: "error" });
      setTimeout(() => setMessage(null), 3000);
    }
  };

  const handleResolve = async (id: string) => {
    try {
      await api.put(`/alerts/${id}/resolve`, {});
      setMessage({ text: "Alert resolved", type: "success" });
      setTimeout(() => setMessage(null), 3000);
      fetchAlerts();
    } catch (e) {
      logger.error("Failed to resolve alert:", e);
      setMessage({ text: "Failed to resolve alert", type: "error" });
      setTimeout(() => setMessage(null), 3000);
    }
  };

  const ago = (dateStr: string) => {
    const diff = Date.now() - new Date(dateStr).getTime();
    const mins = Math.floor(diff / 60000);
    if (mins < 1) return "just now";
    if (mins < 60) return `${mins}m ago`;
    const hrs = Math.floor(mins / 60);
    if (hrs < 24) return `${hrs}h ago`;
    return `${Math.floor(hrs / 24)}d ago`;
  };

  return (
    <div>
      <div className="page-header">
        <div>
          <h1 className="page-header-title">Alerts</h1>
          <p className="page-header-subtitle">Monitor and manage system alerts</p>
        </div>
        <div className="flex items-center gap-2">
          {tab === "alerts" && (
            <button
              onClick={fetchAlerts}
              className="px-3 py-1.5 bg-dark-700 text-dark-200 hover:bg-dark-600 hover:text-dark-100 border border-dark-600 rounded-lg text-sm transition-colors"
            >
              Refresh
            </button>
          )}
        </div>
      </div>

      <div className="p-6 lg:p-8">

      <div className="flex gap-1 mb-6 border-b border-dark-500">
        <button
          onClick={() => setTab("alerts")}
          className={`px-4 py-2 text-sm font-medium border-b-2 -mb-px transition-colors ${
            tab === "alerts"
              ? "border-rust-500 text-rust-400"
              : "border-transparent text-dark-200 hover:text-dark-100"
          }`}
        >
          Alerts
        </button>
        <button
          onClick={() => setTab("runbooks")}
          className={`px-4 py-2 text-sm font-medium border-b-2 -mb-px transition-colors ${
            tab === "runbooks"
              ? "border-rust-500 text-rust-400"
              : "border-transparent text-dark-200 hover:text-dark-100"
          }`}
        >
          Runbooks
        </button>
      </div>

      {message && (
        <div className={`mb-4 px-4 py-3 rounded-lg text-sm border ${
          message.type === "success"
            ? "bg-rust-500/10 text-rust-400 border-rust-500/20"
            : "bg-danger-500/10 text-danger-400 border-danger-500/20"
        }`}>
          {message.text}
        </div>
      )}

      {tab === "alerts" && (
        <>
          {/* Summary cards */}
          <div className="grid grid-cols-1 sm:grid-cols-3 gap-4 mb-6">
            <div
              className={`p-4 rounded-lg border cursor-pointer transition-colors ${
                statusFilter === "firing"
                  ? "bg-danger-500/10 border-danger-500/30"
                  : "bg-dark-800 border-dark-500 hover:border-dark-400"
              }`}
              onClick={() => setStatusFilter(statusFilter === "firing" ? "" : "firing")}
            >
              <div className="text-2xl font-bold text-danger-400">{summary.firing}</div>
              <div className="text-sm text-dark-200">Firing</div>
            </div>
            <div
              className={`p-4 rounded-lg border cursor-pointer transition-colors ${
                statusFilter === "acknowledged"
                  ? "bg-warn-500/10 border-warn-500/30"
                  : "bg-dark-800 border-dark-500 hover:border-dark-400"
              }`}
              onClick={() => setStatusFilter(statusFilter === "acknowledged" ? "" : "acknowledged")}
            >
              <div className="text-2xl font-bold text-warn-400">{summary.acknowledged}</div>
              <div className="text-sm text-dark-200">Acknowledged</div>
            </div>
            <div
              className={`p-4 rounded-lg border cursor-pointer transition-colors ${
                statusFilter === "resolved"
                  ? "bg-rust-500/10 border-rust-500/30"
                  : "bg-dark-800 border-dark-500 hover:border-dark-400"
              }`}
              onClick={() => setStatusFilter(statusFilter === "resolved" ? "" : "resolved")}
            >
              <div className="text-2xl font-bold text-rust-400">{summary.resolved}</div>
              <div className="text-sm text-dark-200">Resolved (30d)</div>
            </div>
          </div>

          {/* Type filter */}
          <div className="flex gap-2 mb-4 flex-wrap">
            <button
              onClick={() => setTypeFilter("")}
              className={`px-3 py-1 rounded-lg text-xs font-medium ${
                !typeFilter ? "bg-rust-500 text-white" : "bg-dark-700 text-dark-200"
              }`}
            >
              All
            </button>
            {Object.entries(TYPE_LABELS).map(([key, label]) => (
              <button
                key={key}
                onClick={() => setTypeFilter(typeFilter === key ? "" : key)}
                className={`px-3 py-1 rounded-lg text-xs font-medium ${
                  typeFilter === key ? "bg-rust-500 text-white" : "bg-dark-700 text-dark-200"
                }`}
              >
                {label}
              </button>
            ))}
          </div>

          {/* Alert list */}
          {loading ? (
            <div className="space-y-3">
              {[1, 2, 3].map((i) => (
                <div key={i} className="h-20 bg-dark-800 rounded-lg animate-pulse" />
              ))}
            </div>
          ) : alerts.length === 0 ? (
            <div className="text-center py-16">
              <svg className="w-12 h-12 mx-auto text-dark-300 mb-3" fill="none" viewBox="0 0 24 24" stroke="currentColor" strokeWidth={1.5}>
                <path strokeLinecap="round" strokeLinejoin="round" d="M14.857 17.082a23.848 23.848 0 0 0 5.454-1.31A8.967 8.967 0 0 1 18 9.75V9A6 6 0 0 0 6 9v.75a8.967 8.967 0 0 1-2.312 6.022c1.733.64 3.56 1.085 5.455 1.31m5.714 0a24.255 24.255 0 0 1-5.714 0m5.714 0a3 3 0 1 1-5.714 0" />
              </svg>
              <p className="text-dark-200 text-sm">
                {statusFilter === "firing"
                  ? "No active alerts -- all systems operational"
                  : "No alerts match the current filters"}
              </p>
            </div>
          ) : (
            <div className="space-y-2">
              {alerts.map((alert) => {
                const sev = SEVERITY_STYLES[alert.severity] || SEVERITY_STYLES.info;
                const isOpen = expanded === alert.id;
                const runbook = runbookCache[alert.alert_type];
                return (
                  <div
                    key={alert.id}
                    className={`rounded-lg border border-dark-500 overflow-hidden ${
                      alert.status === "resolved" ? "bg-dark-800/50 opacity-70" : "bg-dark-800"
                    }`}
                  >
                    <div
                      className="p-4 cursor-pointer hover:bg-dark-700/40"
                      onClick={() => toggleExpand(alert)}
                    >
                      <div className="flex items-start justify-between gap-3">
                        <div className="flex items-start gap-3 min-w-0">
                          <div className={`mt-1 w-2.5 h-2.5 rounded-full shrink-0 ${sev.dot} ${
                            alert.status === "firing" ? "animate-pulse" : ""
                          }`} />
                          <div className="min-w-0">
                            <div className="flex items-center gap-2 mb-1 flex-wrap">
                              <span className="font-medium text-dark-50 text-sm">{alert.title}</span>
                              <span className={`px-1.5 py-0.5 rounded text-xs font-medium ${sev.bg} ${sev.text}`}>
                                {alert.severity}
                              </span>
                              <span className="px-1.5 py-0.5 rounded text-xs bg-dark-700 text-dark-300 font-mono">
                                {TYPE_LABELS[alert.alert_type] || alert.alert_type}
                              </span>
                              <span className="text-xs text-dark-300 ml-1">
                                {isOpen ? "▾" : "▸"} runbook
                              </span>
                            </div>
                            <p className="text-sm text-dark-200 mb-1">{alert.message}</p>
                            <p className="text-xs text-dark-300 font-mono">
                              {ago(alert.created_at)}
                              {alert.resolved_at && ` -- resolved ${ago(alert.resolved_at)}`}
                              {alert.acknowledged_at && !alert.resolved_at && ` -- acknowledged ${ago(alert.acknowledged_at)}`}
                            </p>
                          </div>
                        </div>
                        <div className="flex gap-1.5 shrink-0" onClick={(e) => e.stopPropagation()}>
                          {alert.status === "firing" && (
                            <>
                              <button
                                onClick={() => handleAcknowledge(alert.id)}
                                className="px-2.5 py-1 bg-warn-500/15 text-warn-400 rounded-lg text-xs hover:bg-warn-500/25"
                              >
                                Ack
                              </button>
                              <button
                                onClick={() => handleResolve(alert.id)}
                                className="px-2.5 py-1 bg-rust-500/15 text-rust-400 rounded-lg text-xs hover:bg-rust-500/25"
                              >
                                Resolve
                              </button>
                            </>
                          )}
                          {alert.status === "acknowledged" && (
                            <button
                              onClick={() => handleResolve(alert.id)}
                              className="px-2.5 py-1 bg-rust-500/15 text-rust-400 rounded-lg text-xs hover:bg-rust-500/25"
                            >
                              Resolve
                            </button>
                          )}
                        </div>
                      </div>
                    </div>
                    {isOpen && (
                      <div className="border-t border-dark-500 bg-dark-900/40 px-4 py-3">
                        {runbook === undefined ? (
                          <div className="text-sm text-dark-300">Loading runbook...</div>
                        ) : runbook === null ? (
                          <div className="text-sm text-dark-300">No runbook for this alert type yet. Add one in the Runbooks tab.</div>
                        ) : (
                          <div
                            className="markdown-body text-sm text-dark-100"
                            dangerouslySetInnerHTML={{ __html: renderMarkdown(runbook.runbook_md) }}
                          />
                        )}
                      </div>
                    )}
                  </div>
                );
              })}
            </div>
          )}
        </>
      )}

      {tab === "runbooks" && <RunbooksTab onMessage={(m) => { setMessage(m); setTimeout(() => setMessage(null), 3000); }} />}

      </div>
    </div>
  );
}

function RunbooksTab({ onMessage }: { onMessage: (m: { text: string; type: string }) => void }) {
  const [runbooks, setRunbooks] = useState<Runbook[]>([]);
  const [loading, setLoading] = useState(true);
  const [editing, setEditing] = useState<Runbook | null>(null);
  const [draft, setDraft] = useState("");
  const [draftSeverity, setDraftSeverity] = useState("warning");
  const [seeding, setSeeding] = useState(false);
  const [confirmSeed, setConfirmSeed] = useState(false);

  const fetchRunbooks = async () => {
    try {
      const data = await api.get<Runbook[]>("/alerts/runbooks");
      setRunbooks(data);
    } catch (e) {
      logger.error("Failed to load runbooks:", e);
      onMessage({ text: "Failed to load runbooks", type: "error" });
    } finally {
      setLoading(false);
    }
  };

  useEffect(() => {
    fetchRunbooks();
  }, []);

  const openEdit = (rb: Runbook) => {
    setEditing(rb);
    setDraft(rb.runbook_md);
    setDraftSeverity(rb.severity_default);
  };

  const closeEdit = () => {
    setEditing(null);
    setDraft("");
  };

  const saveEdit = async () => {
    if (!editing) return;
    try {
      await api.put(`/alerts/runbooks/${editing.alert_type}`, {
        runbook_md: draft,
        severity_default: draftSeverity,
      });
      onMessage({ text: `Saved runbook for ${editing.alert_type}`, type: "success" });
      closeEdit();
      fetchRunbooks();
    } catch (e) {
      logger.error("Failed to save runbook:", e);
      onMessage({ text: "Failed to save runbook", type: "error" });
    }
  };

  const restoreDefault = async () => {
    if (!editing) return;
    if (!confirm(`Restore default runbook for ${editing.alert_type}? This discards your edits.`)) return;
    try {
      await api.delete(`/alerts/runbooks/${editing.alert_type}`);
      onMessage({ text: `Restored default for ${editing.alert_type}`, type: "success" });
      closeEdit();
      fetchRunbooks();
    } catch (e) {
      logger.error("Failed to restore default:", e);
      onMessage({ text: "Failed to restore default", type: "error" });
    }
  };

  const seedDefaults = async () => {
    setSeeding(true);
    try {
      const res = await api.post<{ inserted: string[]; skipped: string[] }>("/alerts/runbooks/apply-defaults", {});
      const msg =
        res.inserted.length === 0
          ? `All ${res.skipped.length} runbooks already exist — nothing seeded`
          : `Seeded ${res.inserted.length} runbook${res.inserted.length === 1 ? "" : "s"}` +
            (res.skipped.length ? ` (${res.skipped.length} already customized — skipped)` : "");
      onMessage({ text: msg, type: "success" });
      fetchRunbooks();
    } catch (e) {
      logger.error("Failed to seed runbooks:", e);
      onMessage({ text: "Failed to seed runbooks", type: "error" });
    } finally {
      setSeeding(false);
      setConfirmSeed(false);
    }
  };

  const sortedRunbooks = [...runbooks].sort((a, b) => {
    const sevOrder = { critical: 0, warning: 1, info: 2 } as Record<string, number>;
    const sa = sevOrder[a.severity_default] ?? 3;
    const sb = sevOrder[b.severity_default] ?? 3;
    if (sa !== sb) return sa - sb;
    return a.alert_type.localeCompare(b.alert_type);
  });

  return (
    <div>
      <div className="bg-dark-800 border border-dark-500 rounded-lg p-4 mb-6">
        <div className="flex items-start justify-between gap-4 flex-wrap">
          <div className="min-w-0 max-w-2xl">
            <h2 className="font-medium text-dark-50 mb-1">Runbooks</h2>
            <p className="text-sm text-dark-200">
              Markdown attached to each alert type. Excerpts ride along in slack/discord/pagerduty/webhook payloads, full text appears in incident detail and email. Edit any runbook to customize for your environment — your edits survive panel upgrades.
            </p>
          </div>
          {confirmSeed ? (
            <div className="flex gap-2 items-center bg-dark-700 border border-dark-500 rounded-lg px-3 py-2">
              <span className="text-xs text-dark-200">Seed missing only — won't overwrite edits.</span>
              <button
                onClick={seedDefaults}
                disabled={seeding}
                className="px-2.5 py-1 bg-rust-500 text-white rounded-lg text-xs font-medium hover:bg-rust-600 disabled:opacity-50"
              >
                {seeding ? "Seeding..." : "Confirm"}
              </button>
              <button
                onClick={() => setConfirmSeed(false)}
                className="px-2.5 py-1 bg-dark-700 text-dark-200 rounded-lg text-xs hover:bg-dark-600"
              >
                Cancel
              </button>
            </div>
          ) : (
            <button
              onClick={() => setConfirmSeed(true)}
              className="px-3 py-1.5 bg-rust-500 text-white rounded-lg text-sm font-medium hover:bg-rust-600 shrink-0"
            >
              Seed missing default runbooks
            </button>
          )}
        </div>
      </div>

      {loading ? (
        <div className="space-y-3">
          {[1, 2, 3, 4, 5].map((i) => (
            <div key={i} className="h-16 bg-dark-800 rounded-lg animate-pulse" />
          ))}
        </div>
      ) : (
        <div className="space-y-2">
          {sortedRunbooks.map((rb) => {
            const sev = SEVERITY_STYLES[rb.severity_default] || SEVERITY_STYLES.info;
            return (
              <div key={rb.alert_type} className="p-3 bg-dark-800 border border-dark-500 rounded-lg flex items-center justify-between gap-3">
                <div className="flex items-center gap-3 min-w-0">
                  <div className={`w-2 h-2 rounded-full ${sev.dot}`} />
                  <div className="min-w-0">
                    <div className="flex items-center gap-2 flex-wrap">
                      <span className="font-medium text-dark-50 text-sm">
                        {TYPE_LABELS[rb.alert_type] || rb.alert_type}
                      </span>
                      <span className="px-1.5 py-0.5 rounded text-xs font-mono bg-dark-700 text-dark-300">
                        {rb.alert_type}
                      </span>
                      <span className={`px-1.5 py-0.5 rounded text-xs font-medium ${sev.bg} ${sev.text}`}>
                        {rb.severity_default}
                      </span>
                      {rb.is_default ? (
                        <span className="px-1.5 py-0.5 rounded text-xs bg-dark-700 text-dark-300">default</span>
                      ) : (
                        <span className="px-1.5 py-0.5 rounded text-xs bg-accent-500/10 text-accent-400">customized</span>
                      )}
                    </div>
                  </div>
                </div>
                <button
                  onClick={() => openEdit(rb)}
                  className="px-2.5 py-1 bg-dark-700 text-dark-100 rounded-lg text-xs hover:bg-dark-600 shrink-0"
                >
                  Edit
                </button>
              </div>
            );
          })}
        </div>
      )}

      {editing && (
        <div className="fixed inset-0 bg-black/60 flex items-center justify-center p-4 z-50" onClick={closeEdit}>
          <div className="bg-dark-900 border border-dark-500 rounded-lg max-w-5xl w-full max-h-[90vh] flex flex-col" onClick={(e) => e.stopPropagation()}>
            <div className="px-6 py-4 border-b border-dark-500 flex items-center justify-between">
              <div>
                <h3 className="font-medium text-dark-50">
                  Edit runbook: <span className="font-mono text-sm text-dark-200">{editing.alert_type}</span>
                </h3>
                <p className="text-xs text-dark-300 mt-0.5">
                  {editing.is_default ? "Currently using default — your edits will create a custom version." : "Currently customized."}
                </p>
              </div>
              <button onClick={closeEdit} className="text-dark-300 hover:text-dark-100 text-xl leading-none">×</button>
            </div>
            <div className="flex-1 overflow-hidden grid grid-cols-1 lg:grid-cols-2 gap-0">
              <div className="border-r border-dark-500 flex flex-col">
                <div className="px-4 py-2 bg-dark-800 border-b border-dark-500 flex items-center gap-3">
                  <span className="text-xs font-medium text-dark-200">Markdown</span>
                  <select
                    value={draftSeverity}
                    onChange={(e) => setDraftSeverity(e.target.value)}
                    className="ml-auto bg-dark-700 border border-dark-500 rounded text-xs text-dark-100 px-2 py-1"
                  >
                    <option value="info">info</option>
                    <option value="warning">warning</option>
                    <option value="critical">critical</option>
                  </select>
                </div>
                <textarea
                  value={draft}
                  onChange={(e) => setDraft(e.target.value)}
                  className="flex-1 bg-dark-900 text-dark-100 font-mono text-sm p-4 outline-none resize-none border-0 min-h-[400px]"
                  spellCheck={false}
                />
              </div>
              <div className="flex flex-col bg-dark-800">
                <div className="px-4 py-2 bg-dark-800 border-b border-dark-500">
                  <span className="text-xs font-medium text-dark-200">Preview</span>
                </div>
                <div
                  className="markdown-body flex-1 overflow-y-auto p-4 text-sm text-dark-100 min-h-[400px]"
                  dangerouslySetInnerHTML={{ __html: renderMarkdown(draft) }}
                />
              </div>
            </div>
            <div className="px-6 py-4 border-t border-dark-500 flex items-center justify-between gap-2">
              <div>
                {!editing.is_default && (
                  <button
                    onClick={restoreDefault}
                    className="px-3 py-1.5 bg-dark-700 text-dark-200 hover:bg-dark-600 hover:text-dark-100 border border-dark-500 rounded-lg text-sm"
                  >
                    Restore default
                  </button>
                )}
              </div>
              <div className="flex gap-2">
                <button
                  onClick={closeEdit}
                  className="px-3 py-1.5 bg-dark-700 text-dark-200 hover:bg-dark-600 hover:text-dark-100 border border-dark-500 rounded-lg text-sm"
                >
                  Cancel
                </button>
                <button
                  onClick={saveEdit}
                  className="px-3 py-1.5 bg-rust-500 text-white rounded-lg text-sm font-medium hover:bg-rust-600"
                >
                  Save
                </button>
              </div>
            </div>
          </div>
        </div>
      )}
    </div>
  );
}
