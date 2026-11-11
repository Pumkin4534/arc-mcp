---
name: arc-workflow
description: Use when the operator requests Arc's evidence-gated plan, progress, checks, and independent review workflow for a multi-step coding task.
---

# Arc workflow inside OpenCode

Use the current native OpenCode session and selected model. Do not launch another
planner/writer model or a replacement harness. Keep 1–5 broad ordered outcomes;
retain every acceptance criterion within them. The native todo cap rejects more
than five entries and never truncates scope. Broadness and scope coverage still
require judgment.

With MCP server key `arc`, installed v1 tool names are `arc_arc_plan`,
`arc_arc_status`, `arc_arc_progress`, `arc_arc_run_check`, `arc_arc_revise_task`,
`arc_arc_prepare_review`, and `arc_arc_finalize`. The native plugin tool is
`arc_review`. If the operator uses another server key, use the actual exposed
catalog names (`<server>_<raw-tool-name>`), not guessed aliases.

1. Check status, then plan the full operator request in 1–5 dependency-ordered
   tasks with concrete acceptance details. Preserve the original request; never
   supply a substitute owner identity or pretend the model's summary is the
    host-recorded operator message.
2. Mark the next task working. Use ordinary native OpenCode tools. Keep the todo list aligned without expanding it into
   per-file microtasks. Explicitly revise changed scope with a reason.
3. Run relevant checks through `arc_arc_run_check` for real exit/output evidence.
   Only then mark covered tasks ready. Failed, interrupted, unknown, stale, or
   source-mutating checks do not establish completion. Diagnose before an explicit
   rerun; do not replay side effects automatically.
4. When all tasks are ready, call `arc_review` with `{}`. The native plugin
   obtains the immutable packet and creates two fresh native read-only sessions
   for correctness and adversarial review, using the calling model/variant. Give
   neither reviewer a writer transcript. Ordinary `task` results are not receipts.
5. Address findings and rerun affected checks; request new review only against new
   evidence. Use `arc_arc_finalize` and report its actual result plus caveats. A
   refusal or unavailable native adapter is a blocker, never implied success.

This installation is **native-advisory**. The plugin creates fresh read-only
reviewer sessions, but ordinary same-user native tools can alter the ledger or
invoke the local adapter. Completion records are useful workflow evidence, not
tamper-proof security attestations. No blanket native-tool confinement is applied.
Only MCP-owned checks receive macOS Seatbelt protection: writes to project/scratch,
protected extension/config/state paths denied, no network/Mach delegation. Install
dependencies with normal approved native tools before checking. Checks that need
network or writes outside project/scratch must be reported as blocked, not passed.
Cancellation kills the owned process group; deliberately detached processes are
not guaranteed stopped. Do not use daemon/background checks. Source observations
are scoped and non-atomic; ignored/generated/secret files and symlink referents are
excluded. Report these limits where relevant to acceptance.

Stop/Esc, new operator input, questions, permissions, and explicit blockers take
priority. Arc does not restart an idle/stopped native session. Use `/arc-status`
for an honest blocker report and `/arc-continue` only on explicit operator request.
