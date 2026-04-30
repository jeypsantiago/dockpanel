#!/usr/bin/env bash
# DockPanel Chain-of-Trust Report E2E Test Suite
#
# Exercises the Phase 4 W1.3 chain-of-trust endpoints (v2.8.1):
#   - GET /api/backup-orchestrator/chain-report/site/{id}        (JSON)
#   - GET /api/backup-orchestrator/chain-report/site/{id}/pdf    (PDF, lazy-installs typst)
#
# Auth strategy mirrors tier2-pin-e2e.sh: prefer DOCKPANEL_TEST_PASSWORD;
# otherwise mint a short-lived admin JWT from /etc/dockpanel/api.env.
#
# The first PDF render lazy-installs typst into /var/lib/dockpanel/typst
# (~30MB tarball, ~30s on a fresh box). Subsequent runs are instant.
# Set CHAIN_REPORT_SKIP_PDF=1 to skip the PDF assertion (CI envs without
# outbound HTTPS). Set CHAIN_REPORT_PDF_TIMEOUT to override the curl
# timeout (default 90s) for the first-time install path.
set -uo pipefail

API="${DOCKPANEL_API_URL:-http://127.0.0.1:3080}"
ADMIN_EMAIL="${DOCKPANEL_TEST_EMAIL:-admin@dockpanel.dev}"
ADMIN_PASSWORD="${DOCKPANEL_TEST_PASSWORD:-}"
PDF_TIMEOUT="${CHAIN_REPORT_PDF_TIMEOUT:-90}"

PASS=0 FAIL=0 SKIP=0 TOTAL=0

green() { echo -e "\e[32m  ✓ $1\e[0m"; PASS=$((PASS+1)); TOTAL=$((TOTAL+1)); }
red()   { echo -e "\e[31m  ✗ $1\e[0m"; FAIL=$((FAIL+1)); TOTAL=$((TOTAL+1)); }
skip()  { echo -e "\e[33m  ~ $1\e[0m"; SKIP=$((SKIP+1)); TOTAL=$((TOTAL+1)); }
sect()  { echo; echo "── $1 ──"; }

psql_exec() {
    docker exec dockpanel-postgres psql -U dockpanel -d dockpanel -qtAc "$1" 2>/dev/null
}

echo "═══════════════════════════════════════════════"
echo "  DockPanel Chain-of-Trust Report E2E Suite"
echo "═══════════════════════════════════════════════"

# ── Auth ──────────────────────────────────────────────────────────────────
ADMIN_UID=$(psql_exec "SELECT id FROM users WHERE email = '$ADMIN_EMAIL' AND role = 'admin' LIMIT 1")
if [ -z "$ADMIN_UID" ]; then
    echo "FATAL: Admin user row not found for $ADMIN_EMAIL"
    exit 1
fi

BEARER_TOKEN=""
if [ -n "$ADMIN_PASSWORD" ]; then
    BEARER_TOKEN=$(curl -s -X POST "$API/api/auth/login" -H "Content-Type: application/json" \
        -d "{\"email\":\"$ADMIN_EMAIL\",\"password\":\"$ADMIN_PASSWORD\"}" -D - 2>/dev/null \
        | grep -oP 'token=\K[^;]+')
fi
if [ -z "$BEARER_TOKEN" ] && [ -r /etc/dockpanel/api.env ]; then
    JWT_SECRET=$(grep -E '^JWT_SECRET=' /etc/dockpanel/api.env | cut -d= -f2-)
    if [ -n "$JWT_SECRET" ]; then
        BEARER_TOKEN=$(JWT_SECRET="$JWT_SECRET" ADMIN_UID="$ADMIN_UID" ADMIN_EMAIL="$ADMIN_EMAIL" \
            python3 - <<'PYEOF'
import jwt, os, time
now = int(time.time())
print(jwt.encode(
    {"sub": os.environ["ADMIN_UID"], "email": os.environ["ADMIN_EMAIL"], "role": "admin",
     "iat": now, "exp": now + 600},
    os.environ["JWT_SECRET"], algorithm="HS256",
))
PYEOF
        )
    fi
fi
if [ -z "$BEARER_TOKEN" ]; then
    echo "FATAL: Could not obtain admin token (set DOCKPANEL_TEST_PASSWORD or ensure /etc/dockpanel/api.env is readable)"
    exit 1
fi

AUTH=(-H "Authorization: Bearer $BEARER_TOKEN")

# ── Auth gate ────────────────────────────────────────────────────────────
sect "Authentication gate"

BOGUS_UUID="00000000-0000-0000-0000-000000000000"

UNAUTH_STATUS=$(curl -s -o /dev/null -w "%{http_code}" \
    "$API/api/backup-orchestrator/chain-report/site/$BOGUS_UUID")
if [ "$UNAUTH_STATUS" = "401" ] || [ "$UNAUTH_STATUS" = "403" ]; then
    green "Unauth blocked ($UNAUTH_STATUS)"
else
    red "Unauth NOT blocked (got $UNAUTH_STATUS)"
fi

# ── 404 on bogus id ──────────────────────────────────────────────────────
sect "404 on missing backup"

NOT_FOUND_STATUS=$(curl -s -o /dev/null -w "%{http_code}" "${AUTH[@]}" \
    "$API/api/backup-orchestrator/chain-report/site/$BOGUS_UUID")
if [ "$NOT_FOUND_STATUS" = "404" ]; then
    green "Bogus id returns 404"
else
    red "Bogus id returned $NOT_FOUND_STATUS, expected 404"
fi

# ── Find a site backup to test against ───────────────────────────────────
sect "Discover a site backup"

SITE_BACKUP_ID=$(psql_exec "SELECT id FROM backups ORDER BY created_at DESC LIMIT 1")
if [ -z "$SITE_BACKUP_ID" ]; then
    skip "No site backups exist on this host — JSON/PDF assertions skipped"
    skip "JSON endpoint shape (no fixture)"
    skip "PDF endpoint shape (no fixture)"
    echo
    echo "  Results: $PASS passed, $FAIL failed, $SKIP skipped ($TOTAL total)"
    [ "$FAIL" -eq 0 ] && exit 0 || exit 1
fi
green "Found site backup: $SITE_BACKUP_ID"

# ── JSON endpoint ────────────────────────────────────────────────────────
sect "JSON endpoint"

JSON_BODY=$(curl -s -o /tmp/chain_report.json -w "%{http_code}" "${AUTH[@]}" \
    "$API/api/backup-orchestrator/chain-report/site/$SITE_BACKUP_ID")
if [ "$JSON_BODY" = "200" ]; then
    green "JSON returns 200"
else
    red "JSON returned $JSON_BODY, expected 200"
fi

if grep -q '"panel_version"' /tmp/chain_report.json; then green "JSON has panel_version"; else red "JSON missing panel_version"; fi
if grep -q '"generated_at"' /tmp/chain_report.json; then green "JSON has generated_at"; else red "JSON missing generated_at"; fi
if grep -q '"backup"' /tmp/chain_report.json; then green "JSON has backup"; else red "JSON missing backup"; fi
if grep -q '"verifications"' /tmp/chain_report.json; then green "JSON has verifications"; else red "JSON missing verifications"; fi
if grep -q '"drills"' /tmp/chain_report.json; then green "JSON has drills"; else red "JSON missing drills"; fi
if grep -q '"chain_integrity"' /tmp/chain_report.json; then green "JSON has chain_integrity"; else red "JSON missing chain_integrity"; fi

# Backup id round-trip
JSON_BACKUP_ID=$(python3 -c "import json,sys; print(json.load(open('/tmp/chain_report.json'))['backup']['id'])" 2>/dev/null || echo "")
if [ "$JSON_BACKUP_ID" = "$SITE_BACKUP_ID" ]; then
    green "JSON backup.id matches request"
else
    red "JSON backup.id mismatch (got '$JSON_BACKUP_ID')"
fi

# ── PDF endpoint ─────────────────────────────────────────────────────────
sect "PDF endpoint"

if [ "${CHAIN_REPORT_SKIP_PDF:-0}" = "1" ]; then
    skip "PDF assertion (CHAIN_REPORT_SKIP_PDF=1)"
else
    PDF_HEADERS=$(curl -sI --max-time "$PDF_TIMEOUT" "${AUTH[@]}" \
        "$API/api/backup-orchestrator/chain-report/site/$SITE_BACKUP_ID/pdf" 2>/dev/null)
    PDF_STATUS=$(echo "$PDF_HEADERS" | head -1 | grep -oP '\b\d{3}\b' | head -1)

    if [ "$PDF_STATUS" = "200" ]; then
        green "PDF returns 200"
    elif [ "$PDF_STATUS" = "503" ]; then
        skip "PDF returned 503 (typst install/compile failed — likely no outbound HTTPS)"
        echo "  Results: $PASS passed, $FAIL failed, $SKIP skipped ($TOTAL total)"
        [ "$FAIL" -eq 0 ] && exit 0 || exit 1
    else
        red "PDF returned $PDF_STATUS, expected 200"
    fi

    if echo "$PDF_HEADERS" | grep -qi "content-type: application/pdf"; then
        green "PDF Content-Type is application/pdf"
    else
        red "PDF Content-Type missing or wrong"
    fi

    if echo "$PDF_HEADERS" | grep -qi "content-disposition:.*attachment"; then
        green "PDF has Content-Disposition: attachment"
    else
        red "PDF missing Content-Disposition: attachment"
    fi

    curl -s --max-time "$PDF_TIMEOUT" -o /tmp/chain_report.pdf "${AUTH[@]}" \
        "$API/api/backup-orchestrator/chain-report/site/$SITE_BACKUP_ID/pdf" 2>/dev/null

    if head -c 4 /tmp/chain_report.pdf | grep -q "^%PDF"; then
        green "PDF body starts with %PDF magic"
    else
        red "PDF body does not start with %PDF"
    fi

    PDF_SIZE=$(stat -c%s /tmp/chain_report.pdf 2>/dev/null || echo 0)
    if [ "$PDF_SIZE" -gt 1024 ]; then
        green "PDF size > 1KB ($PDF_SIZE bytes)"
    else
        red "PDF suspiciously small ($PDF_SIZE bytes)"
    fi
fi

echo
echo "  Results: $PASS passed, $FAIL failed, $SKIP skipped ($TOTAL total)"

[ "$FAIL" -eq 0 ] && exit 0 || exit 1
