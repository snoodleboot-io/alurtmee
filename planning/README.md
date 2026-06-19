# Planning

Planning artifacts (PRDs, ARDs, execution plans) for Alurtmee. The directory layout
mirrors a ticket's lifecycle:

| Directory | Holds |
|-----------|-------|
| `current/` | Active, in-flight planning — the ticket(s) being worked right now. |
| `complete/` | Planning for work that has shipped. Moved here once the release lands. |
| `backlog/` | Future ideas not yet scheduled. (Created when first needed.) |

`TICKETS.md` is the ticket registry (local `ALRT-#` scheme) and the source of the
"real" ticket IDs required by the branch-naming convention.

## Lifecycle

1. **Plan** — create the PRD/ARD/exec docs under `current/` (see
   [`.claude/agents/plan-agent.md`](../.claude/agents/plan-agent.md)).
2. **Build** — implement against the plan on a `feat/ALRT-#-*` branch.
3. **Ship & archive** — when the release is cut, `git mv` the delivered planning
   from `current/` into `complete/` and update its row in `TICKETS.md` to `Merged`.

`complete/` currently holds the **v0.1.0** planning (ALRT-1 design + `exec/` phases 0–7).
