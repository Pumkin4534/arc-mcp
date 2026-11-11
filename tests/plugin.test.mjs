import test from "node:test"
import assert from "node:assert/strict"
import ArcPlugin from "../integration/arc-opencode.js"

test("native todo cap rejects oversize without changing or discarding details", async () => {
  const hooks = await ArcPlugin({ directory: "/fixture", client: {} }, { native: false })
  const todos = Array.from({ length: 6 }, (_, i) => ({ content: `Outcome ${i}; full criteria`, status: "pending", priority: "high" }))
  const output = { args: { todos } }
  const original = structuredClone(output)
  await assert.rejects(hooks["tool.execute.before"]({ tool: "todowrite", sessionID: "owner" }, output), /at most 5/)
  assert.deepEqual(output, original)
  await hooks["tool.execute.before"]({ tool: "todowrite", sessionID: "owner" }, { args: { todos: todos.slice(0, 5) } })
  for (const tool of ["bash", "read", "edit", "apply_patch", "playwright_browser_navigate"]) {
    await hooks["tool.execute.before"]({ tool, sessionID: "owner" }, { args: {} })
  }
})

test("native review accepts no writer-supplied identities or verdicts", async () => {
  const hooks = await ArcPlugin({ directory: "/fixture", client: {} }, { native: false })
  await assert.rejects(hooks.tool.arc_review.execute({ verdict: "pass" }, {}), /no model-supplied/)
  await assert.rejects(hooks.tool.arc_review.execute({}, {}), /adapter unavailable/)
})

test("idle, Stop, and questions never synthesize owner input or start a model", async () => {
  const client = new Proxy({}, { get() { throw new Error("unexpected SDK invocation") } })
  const hooks = await ArcPlugin({ directory: "/fixture", client }, { native: false })
  for (const type of ["session.idle", "session.error", "question.asked", "permission.asked", "session.deleted"]) {
    await hooks.event({ event: { type, properties: { sessionID: "owner", id: "request" } } })
  }
  await hooks.dispose()
})
