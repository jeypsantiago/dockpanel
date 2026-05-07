# Container stopped

A managed Docker container is no longer running. Unlike `container_crashloop`, this fires once and indicates a clean stop — either operator-initiated or the container exited and restart policy is `no`.

## Why this fired

The container moved from `running` to `exited`/`dead` and stayed that way. This is informational by default — a planned `docker stop` is normal operational use, so we don't page on it. If the stop was unplanned, the followup signal will usually be a customer-facing 5xx or a `service_down` alert on whatever depended on this container.

## First check

1. From the panel: **Apps → filter Stopped → confirm whether the stop was intentional**.
2. From a terminal:
   ```
   docker ps -a --format '{{.Names}}\t{{.Status}}\t{{.Image}}' | grep -i exited
   docker inspect <container> --format '{{.State.ExitCode}} {{.State.FinishedAt}}'
   docker logs --tail 100 <container>
   ```

## Common causes

- Operator stopped it intentionally (release, migration, debugging)
- Restart policy is `no` and the process exited cleanly
- `docker compose down` ran but only some services were brought back
- Image was pulled and the old container wasn't replaced
- Auto-sleep policy stopped an idle container (this is correct behavior)

## Escalation

Don't page on this alone. Investigate only if a customer reports a problem or if a paired alert fires (`service_down`, dependent containers becoming unhealthy). If the stop was unintentional, restart from the panel and review whether a restart policy should be added.
