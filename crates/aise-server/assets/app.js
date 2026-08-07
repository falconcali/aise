"use strict";

const $ = (id) => document.getElementById(id);

const storyEl = $("story");
const traceViewEl = $("trace-view");
const sessionListEl = $("session-list");
const packListEl = $("pack-list");
const packGridEl = $("pack-grid");
const packsEmptyEl = $("packs-empty");
const importPackBtn = $("import-pack-btn");
const packFileInput = $("pack-file");
const detailHead = $("detail-head");
const detailTitleEl = $("detail-title");
const detailBodyEl = $("detail-body");
const roleListEl = $("role-list");
const rolePickEl = $("role-pick");
const backToPacksBtn = $("back-to-packs");
const turnForm = $("turn-form");
const playerInput = $("player-input");
const sendBtn = $("send-btn");
const traceToggle = $("trace-toggle");
const tabStory = $("tab-story");
const tabTrace = $("tab-trace");
const sceneBox = $("scene-box");
const gameTitleEl = $("game-title");
const gameRoleEl = $("game-role");
const backToDetailBtn = $("back-to-detail");
const roleStateEl = $("role-state");
const worldInfoEl = $("world-info");
const toastEl = $("toast");

let packs = [];
let sessions = [];
let currentView = "packs";
let currentPack = null;
let currentPackJson = null;
let currentSession = null;
let currentStory = null;

function showView(name) {
  currentView = name;
  for (const view of document.querySelectorAll(".view")) {
    view.classList.toggle("active", view.id === `view-${name}`);
  }
}

function toast(message, kind = "info", timeout = 4000) {
  const item = document.createElement("div");
  item.className = `toast-item ${kind}`;
  item.textContent = message;
  toastEl.appendChild(item);
  setTimeout(() => item.remove(), timeout);
}

async function api(path, options) {
  const res = await fetch(path, options);
  if (!res.ok) {
    const raw = await res.text();
    let body = {};
    try {
      body = JSON.parse(raw);
    } catch (_) {
      body = {};
    }
    const error = body.error || raw.trim() || `HTTP ${res.status}`;
    const detail = body.detail || "";
    throw new Error(detail ? `${error}: ${detail}` : error);
  }
  return res;
}

function escapeHtml(value) {
  return String(value).replace(/[&<>"']/g, (c) => ({ "&": "&amp;", "<": "&lt;", ">": "&gt;", '"': "&quot;", "'": "&#39;" }[c]));
}

function playerId() {
  let id = localStorage.getItem("aise.player_id");
  if (!id) {
    id = `player-${crypto.randomUUID()}`;
    localStorage.setItem("aise.player_id", id);
  }
  return id;
}

async function refreshPacks() {
  const res = await api("/api/packs");
  packs = await res.json();
  renderPacks();
  renderPackList();
}

async function refreshSessions() {
  const res = await api("/api/sessions");
  sessions = await res.json();
  renderSessions();
}

function renderPacks() {
  packGridEl.innerHTML = "";
  packsEmptyEl.style.display = packs.length ? "none" : "block";
  for (const pack of packs) {
    const card = document.createElement("div");
    card.className = "pack-card";
    const tags = (pack.tags || []).map((t) => `<span class="tag">${escapeHtml(t)}</span>`).join("");
    card.innerHTML =
      `<div class="pack-title">${escapeHtml(pack.title)}</div>` +
      `<div class="pack-meta">` +
      `<span>v${escapeHtml(pack.version)}</span>` +
      `<span>${escapeHtml(pack.author)}</span>` +
      tags +
      `</div>` +
      `<div class="pack-desc">${escapeHtml(pack.description)}</div>`;
    card.onclick = () => openPackDetail(pack);
    packGridEl.appendChild(card);
  }
}

function renderPackList() {
  packListEl.innerHTML = "";
  for (const pack of packs) {
    const li = document.createElement("li");
    li.innerHTML =
      `<div class="item-main">` +
      `<div class="item-title">${escapeHtml(pack.title)}</div>` +
      `<div class="item-sub">v${escapeHtml(pack.version)}</div>` +
      `</div>`;
    const del = document.createElement("button");
    del.className = "del";
    del.textContent = "×";
    del.title = "删除故事包";
    del.onclick = async (e) => {
      e.stopPropagation();
      await deletePack(pack);
    };
    li.appendChild(del);
    li.onclick = () => openPackDetail(pack);
    packListEl.appendChild(li);
  }
}

function renderSessions() {
  sessionListEl.innerHTML = "";
  for (const s of sessions) {
    const li = document.createElement("li");
    if (currentSession && s.id === currentSession.id) li.classList.add("active");
    li.innerHTML =
      `<div class="item-main">` +
      `<div class="item-title">${escapeHtml(s.name)}</div>` +
      `<div class="item-sub">${new Date(s.created_at).toLocaleString()}</div>` +
      `</div>`;
    const del = document.createElement("button");
    del.className = "del";
    del.textContent = "×";
    del.onclick = async (e) => {
      e.stopPropagation();
      await api(`/api/sessions/${s.id}`, { method: "DELETE" });
      if (currentSession && currentSession.id === s.id) {
        currentSession = null;
        currentStory = null;
        showView("packs");
      }
      await refreshSessions();
    };
    li.appendChild(del);
    li.onclick = () => openGame(s);
    sessionListEl.appendChild(li);
  }
}

importPackBtn.onclick = () => packFileInput.click();
packFileInput.onchange = async () => {
  const file = packFileInput.files[0];
  packFileInput.value = "";
  if (!file) return;
  await importPackFile(file);
};

async function importPackFile(file) {
  if (file.size > 10 * 1024 * 1024) {
    toast("文件过大（超过 10MB）", "error");
    return;
  }
  try {
    const text = await file.text();
    JSON.parse(text);
  } catch (_) {
    toast("文件不是合法的 JSON", "error");
    return;
  }
  try {
    const bytes = new Uint8Array(await file.arrayBuffer());
    const res = await api(`/api/packs/validate?content_type=json`, {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: bytes,
    });
    const report = await res.json();
    if (!report.valid) {
      const issues = (report.issues || [])
        .map((issue) => `<div class="vr-row">[${escapeHtml(issue.code)}] ${escapeHtml(issue.path)} — ${escapeHtml(issue.message)}</div>`)
        .join("");
      toast(`校验未通过，共 ${(report.issues || []).length} 个问题`, "error", 6000);
      const popup = document.createElement("div");
      popup.className = "toast-item error";
      popup.innerHTML = `<strong>校验失败</strong><div class="validation-report">${issues}</div>`;
      toastEl.appendChild(popup);
      setTimeout(() => popup.remove(), 12000);
      return;
    }
    const importRes = await api(`/api/packs?content_type=json`, {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: bytes,
    });
    const imported = await importRes.json();
    toast(`已导入「${file.name}」`, "ok");
    await refreshPacks();
    const found = packs.find((p) => p.pack_id === imported.pack_id);
    if (found) openPackDetail(found);
  } catch (err) {
    toast(`导入失败：${err.message}`, "error", 6000);
  }
}

async function deletePack(pack) {
  if (!confirm(`确定删除故事包「${pack.title}」？`)) return;
  try {
    await api(`/api/packs/${pack.pack_id}`, { method: "DELETE" });
    toast(`已删除「${pack.title}」`, "ok");
    if (currentPack && currentPack.pack_id === pack.pack_id) {
      currentPack = null;
      currentPackJson = null;
      showView("packs");
    }
    await refreshPacks();
  } catch (err) {
    toast(`删除失败：${err.message}`, "error");
  }
}

async function openPackDetail(pack) {
  currentPack = pack;
  showView("detail");
  detailTitleEl.textContent = pack.title;
  detailBodyEl.innerHTML = `<div class="detail-section"><p class="muted">加载中…</p></div>`;
  roleListEl.innerHTML = "";
  rolePickEl.style.display = "none";
  try {
    const res = await api(`/api/packs/${pack.pack_id}`);
    const packJson = await res.json();
    currentPackJson = packJson;
    renderPackDetail(packJson);
  } catch (err) {
    detailBodyEl.innerHTML = `<div class="detail-section"><p class="muted">详情加载失败：${escapeHtml(err.message)}</p></div>`;
  }
}

function renderPackDetail(pack) {
  const meta = pack.meta || {};
  const story = pack.story || {};
  const style = story.style || {};
  const roles = pack.roles || {};
  const play = pack.play || {};
  const start = pack.start || {};
  const worldBook = pack.world_book || {};
  const playableKeys = play.playable_role_keys || [];
  const tags = (meta.tags || []).map((t) => `<span class="tag">${escapeHtml(t)}</span>`).join("");
  const genre = (story.genre || []).map((g) => `<span class="tag">${escapeHtml(g)}</span>`).join("");
  const themes = (story.themes || []).map((t) => `<span class="tag">${escapeHtml(t)}</span>`).join("");

  const characterSection = pack.character_assets && Object.keys(pack.character_assets).length
    ? `<div class="detail-section"><h3>角色卡</h3>` +
      Object.entries(pack.character_assets)
        .map(([key, source]) => {
          if (source && source.character_key) {
            return `<div class="kv"><span class="k">${escapeHtml(key)}</span><span>${escapeHtml(source.character_key)}</span></div>`;
          }
          return "";
        })
        .join("") +
      `</div>`
    : "";

  const worldBookSection = worldBook && worldBook.world_book_key
    ? `<div class="detail-section"><h3>世界书</h3>` +
      `<div class="kv"><span class="k">名称</span><span>${escapeHtml(worldBook.world_book_key)}</span></div>` +
      `<div class="kv"><span class="k">事实</span><span>${Object.keys(worldBook.facts || {}).length} 条</span></div>` +
      `<div class="kv"><span class="k">流言</span><span>${Object.keys(worldBook.rumors || {}).length} 条</span></div>` +
      `</div>`
    : "";

  detailBodyEl.innerHTML =
    `<div class="detail-section"><h3>简介</h3>` +
    `<p>${escapeHtml(meta.description || "")}</p>` +
    `<div class="kv"><span class="k">作者</span><span>${escapeHtml(meta.author || "")}</span></div>` +
    `<div class="kv"><span class="k">版本</span><span>v${escapeHtml(meta.version || "")}</span></div>` +
    `<div class="kv"><span class="k">标签</span><span>${tags}</span></div>` +
    `</div>` +
    `<div class="detail-section"><h3>故事设定</h3>` +
    `<div class="kv"><span class="k">前提</span><span>${escapeHtml(story.premise || "")}</span></div>` +
    `<div class="kv"><span class="k">语言</span><span>${escapeHtml(story.language || "")}</span></div>` +
    `<div class="kv"><span class="k">类型</span><span>${genre}</span></div>` +
    `<div class="kv"><span class="k">主题</span><span>${themes}</span></div>` +
    `<div class="kv"><span class="k">视角</span><span>${escapeHtml(style.point_of_view || "")}</span></div>` +
    `<div class="kv"><span class="k">时态</span><span>${escapeHtml(style.tense || "")}</span></div>` +
    `<div class="kv"><span class="k">基调</span><span>${(style.tone || []).join("、")}</span></div>` +
    `</div>` +
    `<div class="detail-section"><h3>开场</h3><p>${escapeHtml(start.description || "")}</p></div>` +
    characterSection +
    worldBookSection;

  rolePickEl.style.display = "block";
  roleListEl.innerHTML = "";
  if (playableKeys.length === 0) {
    roleListEl.innerHTML = `<p class="muted">该故事包没有可玩角色。</p>`;
    return;
  }
  for (const key of playableKeys) {
    const role = roles[key];
    if (!role) continue;
    const opening = (start.role_openings || {})[key] || "";
    const card = document.createElement("div");
    card.className = "role-card";
    card.innerHTML =
      `<div class="role-label">${escapeHtml(role.role_label || key)}</div>` +
      `<div class="role-fn">${escapeHtml(role.narrative_function || "")}</div>` +
      (opening ? `<div class="role-opening">${escapeHtml(opening)}</div>` : "");
    card.onclick = () => startGame(currentPack, key);
    roleListEl.appendChild(card);
  }
}

async function startGame(pack, roleKey) {
  sendBtn.disabled = true;
  try {
    if (!pack || !pack.pack_id) {
      throw new Error("故事包 ID 缺失，请返回故事包列表后重试");
    }
    const res = await api(`/api/story-instances`, {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify({
        pack_id: pack.pack_id,
        player_id: playerId(),
        player_role_key: roleKey,
      }),
    });
    const instance = await res.json();
    const roleLabel = (currentPackJson?.roles?.[roleKey]?.role_label) || roleKey;
    const sessionRes = await api(`/api/sessions`, {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify({
        name: `${pack.title} · ${roleLabel}`,
        story_id: instance.story_id,
      }),
    });
    const session = await sessionRes.json();
    await refreshSessions();
    await openGame(session);
  } catch (err) {
    toast(`开始游戏失败：${err.message}`, "error", 6000);
  } finally {
    sendBtn.disabled = false;
  }
}

async function openGame(session) {
  currentSession = session;
  currentStory = null;
  showView("game");
  renderSessions();
  gameTitleEl.textContent = session.name;
  gameRoleEl.textContent = "";
  sceneBox.innerHTML = "";
  storyEl.textContent = "";
  resetTraceView();
  roleStateEl.innerHTML = `<p class="muted">加载中…</p>`;
  worldInfoEl.innerHTML = "";
  playerInput.disabled = false;
  sendBtn.disabled = false;
  playerInput.focus();
  await loadStory(session.story_id);
}

async function loadStory(storyId) {
  try {
    const res = await api(`/api/stories/${storyId}`);
    const story = await res.json();
    currentStory = story;
    renderStory();
  } catch (err) {
    toast(`加载故事失败：${err.message}`, "error");
    storyEl.textContent = `加载失败：${err.message}`;
  }
}

function renderStory() {
  const story = currentStory;
  if (!story) return;
  if (story.current_scene) {
    sceneBox.innerHTML = `<div class="scene-label">当前场景</div>${escapeHtml(story.current_scene)}`;
  } else {
    sceneBox.innerHTML = "";
  }
  if (story.player_role_key) {
    const label = currentPackJson?.roles?.[story.player_role_key]?.role_label || story.player_role_key;
    gameRoleEl.textContent = `扮演：${label}`;
  }
  storyEl.textContent = "";
  if ((story.turns || []).length === 0 && story.story_instructions) {
    storyEl.textContent += `${story.story_instructions}\n\n`;
  }
  for (const turn of story.turns || []) {
    if (turn.player_input) {
      storyEl.textContent += `\n你：${turn.player_input}\n\n`;
    }
    storyEl.textContent += `${turn.story_text}\n\n`;
  }
  storyEl.scrollTop = storyEl.scrollHeight;
  renderRoleState(story);
  renderWorldInfo(story);
}

function renderRoleState(story) {
  roleStateEl.innerHTML = "";
  const characters = story.characters || [];
  if (characters.length === 0) {
    roleStateEl.innerHTML = `<p class="muted">暂无角色状态</p>`;
    return;
  }
  for (const character of characters) {
    const card = document.createElement("div");
    card.className = "state-card";
    const isPlayer = story.player_role_key && character.role_key === story.player_role_key;
    const attrs = (character.attributes || [])
      .map((a) => `<div class="state-attr"><span>${escapeHtml(a.key)}</span><span>${escapeHtml(a.value)}</span></div>`)
      .join("");
    card.innerHTML =
      `<div class="state-name">${escapeHtml(character.role_key)}${isPlayer ? "（你）" : ""}</div>` +
      `<div class="state-row">位置：${escapeHtml(character.location)}</div>` +
      (character.goals && character.goals.length
        ? `<div class="state-row">目标：${character.goals.map(escapeHtml).join("；")}</div>`
        : "") +
      attrs;
    roleStateEl.appendChild(card);
  }
}

function renderWorldInfo(story) {
  worldInfoEl.innerHTML = `<p class="muted">故事进行中的世界信息将在此展示。</p>`;
}

function resetTraceView() {
  traceViewEl.innerHTML = '<div class="trace-empty">勾选“调试 Trace”以生成</div>';
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

backToPacksBtn.onclick = () => {
  currentPack = null;
  currentPackJson = null;
  showView("packs");
};

backToDetailBtn.onclick = () => {
  if (currentPack) {
    openPackDetail(currentPack);
  } else {
    showView("packs");
  }
};

turnForm.onsubmit = async (e) => {
  e.preventDefault();
  if (!currentSession) return;
  const input = playerInput.value.trim();
  if (!input) return;

  playerInput.disabled = true;
  sendBtn.disabled = true;
  storyEl.textContent += `\n你：${input}\n\n`;
  storyEl.scrollTop = storyEl.scrollHeight;
  const traceEnabled = traceToggle.checked;
  traceViewEl.innerHTML = traceEnabled
    ? '<div class="trace-empty">正在生成 Trace…</div>'
    : '<div class="trace-empty">勾选“调试 Trace”以生成</div>';

  try {
    const res = await api(`/api/sessions/${currentSession.id}/turns`, {
      method: "POST",
      headers: {
        "Content-Type": "application/json",
        "Idempotency-Key": crypto.randomUUID(),
      },
      body: JSON.stringify({ player_input: input, include_trace: traceEnabled }),
    });
    await consumeSse(res.body, {
      onStage: (stage) => console.debug("[stage]", stage),
      onCommitted: async (result) => {
        if (result.story_text) {
          storyEl.textContent += `\n${result.story_text}\n\n`;
          storyEl.scrollTop = storyEl.scrollHeight;
        }
        await loadStory(currentSession.story_id);
      },
      onFailed: (payload) => {
        const text = payload && payload.code ? payload.code : JSON.stringify(payload);
        storyEl.textContent += `\n[失败] ${text}\n`;
      },
      onCancelled: (payload) => {
        const text = payload && payload.code ? payload.code : JSON.stringify(payload);
        storyEl.textContent += `\n[已取消] ${text}\n`;
      },
      onConflict: (payload) => {
        const text = payload && payload.code ? payload.code : JSON.stringify(payload);
        storyEl.textContent += `\n[冲突] ${text}\n`;
      },
      onTrace: (trace) => renderTrace(trace),
    });
  } catch (err) {
    storyEl.textContent += `\n[错误] ${err.message}\n`;
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
  if (event === "stage") handlers.onStage?.(data);
  else if (event === "validation") handlers.onStage?.(`validation:${data}`);
  else if (event === "done") handlers.onDone?.(data);
  else if (event === "committed") {
    try {
      handlers.onCommitted?.(JSON.parse(data));
    } catch (_) {
      /* ignore malformed committed payload */
    }
  } else if (event === "failed") handlers.onFailed?.(JSON.parse(data));
  else if (event === "cancelled") handlers.onCancelled?.(JSON.parse(data));
  else if (event === "conflict") handlers.onConflict?.(JSON.parse(data));
  else if (event === "trace") {
    try {
      handlers.onTrace?.(JSON.parse(data));
    } catch (_) {
      /* ignore malformed trace payload */
    }
  }
}

function renderTrace(trace) {
  traceViewEl.innerHTML = "";

  const header = document.createElement("div");
  header.className = "trace-header";
  const started = new Date(trace.started_at_ms || Date.now());
  header.innerHTML =
    `<strong>Trace</strong> <code>${escapeHtml(trace.trace_id || "")}</code>` +
    `<span>Turn: <code>${escapeHtml(trace.turn_id || "")}</code></span>` +
    `<span>开始: ${started.toLocaleString()}</span>` +
    (trace.duration_ms ? `<span>耗时: ${trace.duration_ms} ms</span>` : "");
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

  if (!trace.spans || trace.spans.length === 0) {
    const empty = document.createElement("div");
    empty.className = "trace-empty";
    empty.textContent = "Trace 已生成，但未返回详细 span 数据。";
    traceViewEl.appendChild(empty);
    return;
  }

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

showView("packs");
refreshPacks();
refreshSessions();
