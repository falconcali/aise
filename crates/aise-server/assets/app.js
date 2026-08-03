"use strict";

const storyEl = document.getElementById("story");
const sessionListEl = document.getElementById("session-list");
const newSessionForm = document.getElementById("new-session-form");
const sessionNameInput = document.getElementById("session-name");
const turnForm = document.getElementById("turn-form");
const playerInput = document.getElementById("player-input");
const sendBtn = document.getElementById("send-btn");

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
  playerInput.disabled = false;
  sendBtn.disabled = false;
  playerInput.focus();
}

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

  try {
    const res = await api(`/api/sessions/${currentSessionId}/turns`, {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify({ player_input: input }),
    });
    await consumeSse(res.body, {
      onToken: (text) => appendText(text),
      onStage: (stage) => console.debug("[stage]", stage),
      onDone: () => appendText("\n"),
    });
  } catch (err) {
    appendText(`\n[错误] ${err.message}\n`);
  } finally {
    playerInput.disabled = false;
    sendBtn.disabled = false;
    playerInput.value = "";
    playerInput.focus();
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
}

function appendText(text) {
  storyEl.textContent += text;
  storyEl.scrollTop = storyEl.scrollHeight;
}

refreshSessions();
