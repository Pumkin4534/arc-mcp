// Real installed OpenCode + real MCP/plugin, with a deterministic LOCAL provider.
// Exercises transport/hooks/native sessions; does not claim LLM review quality.
import assert from "node:assert/strict"
import { createServer } from "node:http"
import { spawn, execFileSync } from "node:child_process"
import { mkdtemp, mkdir, writeFile, readFile, copyFile, chmod } from "node:fs/promises"
import { dirname, join, resolve } from "node:path"
import { fileURLToPath } from "node:url"
import { homedir } from "node:os"

const repo = resolve(dirname(fileURLToPath(import.meta.url)), "..")
const builtBinary = process.env.ARC_TEST_BINARY ?? join(repo, "target/debug/arc-mcp")
const opencode = join(homedir(), ".opencode/bin/opencode")
const integrationRoot = process.env.ARC_TEST_INTEGRATION ?? join(repo, "integration")
const fixture = await mkdtemp(join(repo, "target/native-smoke-"))
const binary = join(fixture, "arc-mcp")
await copyFile(builtBinary, binary) // Cargo target executables can be hardlinked.
await chmod(binary, 0o755)
const project = join(fixture, "project")
const home = join(fixture, "home")
const dataHome = join(fixture, "arc-state")
await mkdir(project, { recursive: true })
await mkdir(home, { recursive: true })
await writeFile(join(project, "answer.txt"), "42\n")
const steps = [
  ["arc_arc_plan", { goal: "Verify answer 42", tasks: [{ id: "verify", title: "Verify the answer", acceptance: ["answer.txt contains exactly 42", "A real check and two fresh reviews are recorded"], depends_on: [] }] }],
  ["arc_arc_run_check", { task_ids: ["verify"], command: 'test "$(cat answer.txt)" = 42 && printf "answer verified\\n"', timeout_secs: 5 }],
  ["arc_arc_progress", { task_id: "verify", status: "ready", reason: "Actual answer check passed" }],
  ["arc_review", {}],
  ["arc_arc_finalize", {}],
]
const requests = []
const failures = []
const stopScenario = process.env.ARC_TEST_SCENARIO === "stop"
let reviewRequests = 0
function text(content) { return typeof content === "string" ? content : (content ?? []).map((p) => p.text ?? "").join("\n") }
const provider = createServer(async (req, res) => {
  try {
    const chunks = []
    for await (const chunk of req) chunks.push(chunk)
    const body = JSON.parse(Buffer.concat(chunks))
    requests.push(body)
    const system = body.messages.filter((m) => m.role === "system").map((m) => text(m.content)).join("\n")
    let message
    if (system.includes("You are the independent Arc")) {
      reviewRequests++
      const users = body.messages.filter((m) => m.role === "user")
      assert.equal(users.length, 1, "review receives only packet user message")
      const packet = JSON.parse(text(users[0].content))
      assert.equal(packet.assurance, "native-advisory")
      const permitted = (body.tools ?? []).map((tool) => tool.function.name)
      assert.ok(permitted.every((name) => ["read", "glob", "grep"].includes(name)), `reviewer tools: ${permitted}`)
      message = { role: "assistant", content: JSON.stringify({ verdict: "pass", task_ids: packet.task_ids, evidence_ids: packet.evidence_ids, findings: ["Deterministic transport fixture; not a semantic LLM judgment."] }) }
      if (stopScenario) {
        // Leave both real reviewer sessions in-flight until native owner Stop.
        const timer = setTimeout(() => res.end(), 90000)
        res.on("close", () => clearTimeout(timer))
        return
      }
    } else {
      const prior = body.messages.filter((m) => m.role === "assistant").flatMap((m) => m.tool_calls ?? [])
      const step = steps[prior.length]
      if (step) {
        const tools = (body.tools ?? []).map((tool) => tool.function.name)
        assert.ok(tools.includes(step[0]), `missing ${step[0]}; catalog=${tools}`)
        assert.ok(tools.includes("bash") && tools.includes("read"), "native tools remain available")
        message = { role: "assistant", content: null, tool_calls: [{ id: `fixture_${prior.length}`, type: "function", function: { name: step[0], arguments: JSON.stringify(step[1]) } }] }
      } else message = { role: "assistant", content: "Native Arc integration fixture finished." }
    }
    const base = { id: `fixture-${requests.length}`, created: Math.floor(Date.now() / 1000), model: "fixture" }
    const finish = message.tool_calls ? "tool_calls" : "stop"
    if (body.stream) {
      res.writeHead(200, { "content-type": "text/event-stream" })
      const delta = message.tool_calls ? { role: "assistant", tool_calls: message.tool_calls.map((t, index) => ({ index, ...t })) } : message
      res.write(`data: ${JSON.stringify({ ...base, object: "chat.completion.chunk", choices: [{ index: 0, delta, finish_reason: null }] })}\n\n`)
      res.write(`data: ${JSON.stringify({ ...base, object: "chat.completion.chunk", choices: [{ index: 0, delta: {}, finish_reason: finish }], usage: { prompt_tokens: 100, completion_tokens: 30, total_tokens: 130 } })}\n\n`)
      res.end("data: [DONE]\n\n")
    } else {
      res.writeHead(200, { "content-type": "application/json" })
      res.end(JSON.stringify({ ...base, object: "chat.completion", choices: [{ index: 0, message, finish_reason: finish }], usage: { prompt_tokens: 100, completion_tokens: 30, total_tokens: 130 } }))
    }
  } catch (error) {
    failures.push(String(error.stack))
    res.writeHead(400, { "content-type": "application/json" })
    res.end(JSON.stringify({ error: { message: String(error), type: "fixture_failure" } }))
  }
})
await new Promise((done) => provider.listen(0, "127.0.0.1", done))
const config = {
  $schema: "https://opencode.ai/config.json", model: "fixture/fixture", small_model: "fixture/fixture",
  enabled_providers: ["fixture"], autoupdate: false, snapshot: false, share: "disabled", permission: "allow", lsp: false,
  provider: { fixture: { npm: "@ai-sdk/openai-compatible", name: "Local deterministic fixture", options: { baseURL: `http://127.0.0.1:${provider.address().port}/v1` }, models: { fixture: { name: "fixture", tool_call: true, limit: { context: 128000, output: 8192 } } } } },
  mcp: { arc: { type: "local", command: [binary, "--data-home", dataHome, "serve"], timeout: 90000, enabled: true } },
  plugin: [[join(integrationRoot, process.env.ARC_TEST_INTEGRATION ? "plugins/arc-opencode.js" : "arc-opencode.js"), { binary, dataHome }]],
}
if (process.env.ARC_TEST_INSTALLER === "1") {
  delete config.mcp
  delete config.plugin
}
await mkdir(join(home, ".config/opencode"), { recursive: true })
for (const relative of ["agents/arc-reviewer.md", "commands/arc.md", "commands/arc-status.md", "commands/arc-continue.md", "skills/arc-workflow/SKILL.md"]) {
  const target = join(home, ".config/opencode", relative)
  await mkdir(dirname(target), { recursive: true })
  await copyFile(join(integrationRoot, relative), target)
}
await writeFile(join(home, ".config/opencode/opencode.json"), JSON.stringify(config))
const env = {
  PATH: process.env.PATH, HOME: home, OPENCODE_TEST_HOME: home,
  XDG_CONFIG_HOME: join(home, ".config"), XDG_DATA_HOME: join(home, ".local/share"), XDG_CACHE_HOME: join(home, ".cache"), XDG_STATE_HOME: join(home, ".local/state"),
  OPENCODE_DISABLE_DEFAULT_PLUGINS: "1", OPENCODE_DISABLE_PROJECT_CONFIG: "1", OPENCODE_DISABLE_EXTERNAL_SKILLS: "1", OPENCODE_DISABLE_CLAUDE_CODE: "1", OPENCODE_DISABLE_MODELS_FETCH: "1", OPENCODE_DISABLE_AUTOUPDATE: "1",
}
if (process.env.ARC_TEST_INSTALLER === "1") {
  execFileSync(process.execPath, [join(repo, "scripts/install.mjs"), "--install"], {
    cwd: repo, env: { ...env, ARC_CONFIG_HOME: join(home, ".config/opencode"), ARC_INSTALL_BINARY: binary }, encoding: "utf8",
  })
}
const child = spawn(opencode, ["serve", "--hostname", "127.0.0.1", "--port", "0"], { cwd: project, env, stdio: ["ignore", "pipe", "pipe"] })
let log = ""
child.stdout.on("data", (data) => { log += data })
child.stderr.on("data", (data) => { log += data })
const started = Date.now()
try {
  let base
  while (!(base = log.match(/http:\/\/127\.0\.0\.1:\d+/)?.[0])) {
    assert.equal(child.exitCode, null, log)
    assert.ok(Date.now() - started < 60000, `OpenCode startup timeout: ${log}`)
    await new Promise((done) => setTimeout(done, 50))
  }
  async function api(path, body) {
    const response = await fetch(`${base}${path}?directory=${encodeURIComponent(project)}`, { method: body === undefined ? "GET" : "POST", headers: { "content-type": "application/json" }, ...(body === undefined ? {} : { body: JSON.stringify(body) }), signal: AbortSignal.timeout(120000) })
    const text = await response.text()
    assert.ok(response.ok, `${path}: ${response.status} ${text}`)
    return JSON.parse(text)
  }
  const resolved = await api("/config")
  assert.ok(resolved.mcp.arc.enabled)
  assert.ok(resolved.command.arc.template.includes("$ARGUMENTS"))
  assert.equal(resolved.agent["arc-reviewer"].permission["*"], "deny")
  const status = await api("/mcp")
  assert.equal(status.arc.status, "connected", JSON.stringify(status))
  const session = await api("/session", { title: "Arc native integration fixture" })
  const active = api(`/session/${session.id}/command`, { command: "arc", arguments: "Verify answer.txt contains 42 using the Arc workflow and two native reviewers.", model: "fixture/fixture" })
  // Attach immediately so cancellation cannot produce an unhandled rejection.
  const settled = active.then((value) => ({ value }), (error) => ({ error }))
  if (stopScenario) {
    const wait = Date.now()
    while (reviewRequests < 2) {
      assert.ok(Date.now() - wait < 60000, `Review launch timeout: ${failures}`)
      await new Promise((done) => setTimeout(done, 50))
    }
    await api(`/session/${session.id}/abort`, {})
  }
  const finalResult = await settled
  if (!stopScenario && finalResult.error) throw finalResult.error
  const final = finalResult.value
  const messages = await api(`/session/${session.id}/message`)
  await writeFile(join(fixture, "messages.json"), JSON.stringify(messages, null, 2))
  const tools = messages.flatMap((m) => m.parts).filter((p) => p.type === "tool")
  assert.deepEqual(tools.map((t) => t.tool), steps.slice(0, stopScenario ? 4 : 5).map(([name]) => name))
  for (const tool of tools.slice(0, stopScenario ? 3 : 5)) assert.equal(tool.state.status, "completed", JSON.stringify(tool))
  if (!stopScenario) {
    const result = JSON.parse(tools.at(-1).state.output)
    assert.equal(result.complete, true, JSON.stringify(tools, null, 2))
    assert.equal(result.assurance, "native-advisory")
    assert.equal(result.tamper_protected, false)
  } else {
    assert.equal(tools.at(-1).state.status, "error", JSON.stringify(tools.at(-1)))
    const count = requests.length
    await new Promise((done) => setTimeout(done, 500))
    assert.equal(requests.length, count, "Stop must not restart model requests")
    const statuses = await api("/session/status")
    assert.ok(!statuses[session.id] || statuses[session.id].type === "idle")
  }
  const children = await api(`/session/${session.id}/children`)
  assert.equal(children.length, 2, JSON.stringify(children))
  assert.notEqual(children[0].id, children[1].id)
  for (const reviewer of children) {
    const transcript = await api(`/session/${reviewer.id}/message`)
    assert.equal(transcript.filter((m) => m.info.role === "user").length, 1)
    assert.equal(transcript.filter((m) => m.info.role === "assistant").length, 1)
    if (stopScenario) assert.ok(transcript.at(-1).info.error, "Stopped reviewer has no passing judgment")
    assert.deepEqual(reviewer.permission.map((p) => p.permission), ["*", "read", "glob", "grep"])
  }
  assert.equal(failures.length, 0, failures.join("\n"))
  if (!stopScenario) assert.equal(final.info.error, undefined)
  assert.equal(await readFile(join(project, "answer.txt"), "utf8"), "42\n")
  console.log(JSON.stringify({ passed: true, scenario: stopScenario ? "native Stop cancels both reviewers" : "complete workflow via /arc", version: "OpenCode 1.18.25", transport: "real installed OpenCode/native sessions/MCP/plugin", provider: "local deterministic fixture, no live LLM", owner: session.id, reviewers: children.map((s) => s.id), requests: requests.length, elapsed_ms: Date.now() - started, fixture }))
} finally {
  await writeFile(join(fixture, "opencode.log"), log)
  await writeFile(join(fixture, "provider-failures.json"), JSON.stringify(failures, null, 2))
  child.kill("SIGTERM")
  const timer = setTimeout(() => child.kill("SIGKILL"), 3000)
  timer.unref()
  await new Promise((done) => child.exitCode !== null ? done() : child.once("exit", done))
  clearTimeout(timer)
  provider.closeAllConnections()
  await new Promise((done) => provider.close(done))
  console.log(`Fixture artifacts: ${fixture}`)
}
