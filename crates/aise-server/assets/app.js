"use strict";

const storyEl = document.getElementById("story");
const traceViewEl = document.getElementById("trace-view");
const sessionListEl = document.getElementById("session-list");
const newSessionForm = document.getElementById("new-session-form");
const sessionNameInput = document.getElementById("session-name");
const turnForm = document.getElementById("turn-form");
const playerInput = document.getElementById("player-input");
const sendBtn = document.getElementById("send-btn");
const traceToggle = document.getElementById("trace-toggle");
const tabStory = document.getElementById("tab-story");
const tabTrace = document.getElementById("tab-trace");

let currentSessionId = null;
let sessions = [];

async function api(path, options) {
  const res = await fetch(path, options);
  if (!res.ok) {
    const body = await res.json().catch(() => ({}));
    throw new Error(body.error || `HTTP ${res.status}`);
  }
  return res;
}

async function refreshSessions() {
  const res = await api("/api/sessions");
  sessions = await res.json();
  renderSessions();
}

function renderSessions() {
  sessionListEl.innerHTML = "";
  for (const s of sessions) {
    const li = document.createElement("li");
    li.textContent = s.name;
    if (s.id === currentSessionId) li.classList.add("active");

    const del = document.createElement("button");
    del.className = "del";
    del.textContent = "×";
    del.onclick = async (e) => {
      e.stopPropagation();
      await api(`/api/sessions/${s.id}`, { method: "DELETE" });
      if (currentSessionId === s.id) {
        currentSessionId = null;
        storyEl.textContent = "";
        playerInput.disabled = true;
        sendBtn.disabled = true;
      }
      await refreshSessions();
    };
    li.appendChild(del);

    li.onclick = () => selectSession(s.id);
    sessionListEl.appendChild(li);
  }
}

function selectSession(id) {
  currentSessionId = id;
  renderSessions();
  storyEl.textContent = "";
  resetTraceView();
  playerInput.disabled = false;
  sendBtn.disabled = false;
  playerInput.focus();
}

function switchTab(tab) {
  const isTrace = tab === "trace";
  tabStory.classList.toggle("active", !isTrace);
  tabTrace.classList.toggle("active", isTrace);
  storyEl.classList.toggle("active", !isTrace);
  traceViewEl.classList.toggle("active", isTrace);
}

tabStory.onclick = () => switchTab("story");
tabTrace.onclick = () => switchTab("trace");

newSessionForm.onsubmit = async (e) => {
  e.preventDefault();
  const name = sessionNameInput.value.trim();
  if (!name) return;
  const res = await api("/api/sessions", {
    method: "POST",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify({ name }),
  });
  const session = await res.json();
  sessionNameInput.value = "";
  await refreshSessions();
  selectSession(session.id);
};

// Turn submission: POST returns an SSE stream that we parse manually.
turnForm.onsubmit = async (e) => {
  e.preventDefault();
  if (!currentSessionId) return;
  const input = playerInput.value.trim();
  if (!input) return;

  playerInput.disabled = true;
  sendBtn.disabled = true;
  appendText("\n\n" + input + "\n\n");
  const traceEnabled = traceToggle.checked;
  traceViewEl.innerHTML = traceEnabled
    ? '<div class="trace-empty">正在生成 Trace…</div>'
    : '<div class="trace-empty">勾选“调试 Trace”以生成</div>';

  try {
    const res = await api(`/api/sessions/${currentSessionId}/turns`, {
      method: "POST",
      headers: {
        "Content-Type": "application/json",
        "Idempotency-Key": crypto.randomUUID(),
      },
      body: JSON.stringify({ player_input: input, include_trace: traceToggle.checked }),
    });
    await consumeSse(res.body, {
      onToken: (text) => appendText(text),
      onStage: (stage) => console.debug("[stage]", stage),
      onDone: () => appendText("\n"),
      onTrace: (trace) => renderTrace(trace),
    });
  } catch (err) {
    appendText(`\n[错误] ${err.message}\n`);
    if (traceEnabled) {
      traceViewEl.innerHTML = '<div class="trace-empty">生成 Trace 失败，见上方错误信息</div>';
    }
  } finally {
    playerInput.disabled = false;
    sendBtn.disabled = false;
    playerInput.value = "";
    playerInput.focus();
    if (traceEnabled && traceViewEl.querySelector(".trace-empty")?.textContent.includes("正在生成")) {
      traceViewEl.innerHTML = '<div class="trace-empty">未收到 Trace 数据（请求异常中断）</div>';
    }
  }
};

async function consumeSse(body, handlers) {
  const reader = body.getReader();
  const decoder = new TextDecoder();
  let buf = "";
  while (true) {
    const { done, value } = await reader.read();
    if (done) break;
    buf += decoder.decode(value, { stream: true });
    let idx;
    while ((idx = buf.indexOf("\n\n")) !== -1) {
      const rawEvent = buf.slice(0, idx);
      buf = buf.slice(idx + 2);
      parseSseEvent(rawEvent, handlers);
    }
  }
}

function parseSseEvent(raw, handlers) {
  let event = "message";
  const dataLines = [];
  for (const line of raw.split("\n")) {
    if (line.startsWith("event:")) event = line.slice(6).trim();
    else if (line.startsWith("data:")) dataLines.push(line.slice(5).trim());
  }
  if (dataLines.length === 0) return;
  const data = dataLines.join("\n");
  if (event === "token") handlers.onToken?.(data);
  else if (event === "stage") handlers.onStage?.(data);
  else if (event === "validation") handlers.onStage?.(`validation:${data}`);
  else if (event === "done") handlers.onDone?.(data);
  else if (event === "trace") {
    try {
      handlers.onTrace?.(JSON.parse(data));
    } catch (_) {
      /* ignore malformed trace payload */
    }
  }
}

function appendText(text) {
  storyEl.textContent += text;
  storyEl.scrollTop = storyEl.scrollHeight;
}

function escapeHtml(value) {
  return String(value).replace(/[&<>"']/g, (c) => ({ "&": "&amp;", "<": "&lt;", ">": "&gt;", '"': "&quot;", "'": "&#39;" }[c]));
}

function resetTraceView() {
  traceViewEl.innerHTML = '<div class="trace-empty">勾选“调试 Trace”以生成</div>';
}

function renderTrace(trace) {
  traceViewEl.innerHTML = "";

  const header = document.createElement("div");
  header.className = "trace-header";
  const started = new Date(trace.started_at_ms);
  header.innerHTML =
    `<strong>Trace</strong> <code>${escapeHtml(trace.trace_id)}</code>` +
    `<span>Turn: <code>${escapeHtml(trace.turn_id)}</code></span>` +
    `<span>Story: <code>${escapeHtml(trace.story_id)}</code></span>` +
    `<span>开始: ${started.toLocaleString()}</span>` +
    `<span>耗时: ${trace.duration_ms} ms</span>` +
    `<span>Spans: ${trace.spans.length}${trace.dropped_span_count ? ` (+${trace.dropped_span_count} 丢弃)` : ""}</span>`;
  traceViewEl.appendChild(header);

  const actions = document.createElement("div");
  actions.className = "trace-actions";
  const expandAll = document.createElement("button");
  expandAll.textContent = "展开全部";
  expandAll.onclick = () => traceViewEl.querySelectorAll("details").forEach((d) => (d.open = true));
  const collapseAll = document.createElement("button");
  collapseAll.textContent = "收起全部";
  collapseAll.onclick = () => traceViewEl.querySelectorAll("details").forEach((d) => (d.open = false));
  actions.appendChild(expandAll);
  actions.appendChild(collapseAll);
  traceViewEl.appendChild(actions);

  const byParent = new Map();
  for (const s of trace.spans) {
    const key = s.parent_span_id || "";
    if (!byParent.has(key)) byParent.set(key, []);
    byParent.get(key).push(s);
  }
  const roots = byParent.get("") || [];
  const list = document.createElement("div");
  list.className = "trace-tree";
  for (const root of roots) renderSpanNode(root, byParent, list);
  traceViewEl.appendChild(list);
}

function renderSpanNode(span, byParent, parentEl) {
  const details = document.createElement("details");
  details.className = `trace-span ${span.kind}`;

  const summary = document.createElement("summary");
  const payload = span.payload && typeof span.payload === "object" ? span.payload : {};
  const status = payload.status ? `<span class="status ${escapeHtml(payload.status)}">${escapeHtml(payload.status)}</span>` : "";
  const duration = span.duration_ms > 0 ? `<span class="dur">${span.duration_ms} ms</span>` : "";
  summary.innerHTML =
    `<span class="kind">${escapeHtml(span.kind)}</span>` +
    `<span class="name">${escapeHtml(span.name)}</span>` +
    status +
    duration;
  details.appendChild(summary);

  const body = document.createElement("div");
  body.className = "trace-body";
  const pre = document.createElement("pre");
  pre.textContent = JSON.stringify(span.payload, null, 2);
  body.appendChild(pre);

  const children = byParent.get(span.span_id) || [];
  for (const child of children) renderSpanNode(child, byParent, body);

  details.appendChild(body);
  parentEl.appendChild(details);
}

refreshSessions();
