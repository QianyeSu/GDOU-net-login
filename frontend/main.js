const state = {
  taskRunning: false,
};

const ids = [
  "username",
  "password",
  "portal_url",
  "probe_url",
  "ac_id",
  "retry_seconds",
  "auto_query_acid",
  "auto_reconnect",
  "show_password",
  "os_name",
  "device_name",
  "n",
  "login_type",
];

for (const id of ids) {
  document.getElementById(id).addEventListener("input", persistDraft);
  document.getElementById(id).addEventListener("change", persistDraft);
}

document.getElementById("show_password").addEventListener("change", (e) => {
  document.getElementById("password").type = e.target.checked ? "text" : "password";
});

document.getElementById("save").addEventListener("click", () => invoke("save_config_cmd"));
document.getElementById("login").addEventListener("click", () => invoke("login_cmd"));
document.getElementById("logout").addEventListener("click", () => invoke("logout_cmd"));
document.getElementById("status").addEventListener("click", () => invoke("check_status_cmd"));
document.getElementById("auto_reconnect").addEventListener("change", (e) => {
  invoke("set_auto_reconnect_cmd", { enabled: e.target.checked });
});

function getPayload(includePassword = true) {
  return {
    portal_url: value("portal_url"),
    probe_url: value("probe_url"),
    username: value("username"),
    password: includePassword ? value("password") : "",
    ac_id: value("ac_id"),
    retry_seconds: Number(value("retry_seconds") || 30),
    auto_query_acid: checked("auto_query_acid"),
    auto_reconnect: checked("auto_reconnect"),
    os_name: value("os_name"),
    device_name: value("device_name"),
    n: Number(value("n") || 200),
    login_type: Number(value("login_type") || 1),
  };
}

function value(id) {
  return document.getElementById(id).value;
}

function checked(id) {
  return document.getElementById(id).checked;
}

function setValue(id, v) {
  document.getElementById(id).value = v ?? "";
}

function setChecked(id, v) {
  document.getElementById(id).checked = !!v;
}

function applyConfig(config) {
  for (const [k, v] of Object.entries(config)) {
    const el = document.getElementById(k);
    if (!el) continue;
    if (el.type === "checkbox") setChecked(k, v);
    else setValue(k, v);
  }
  document.getElementById("password").type = checked("show_password") ? "text" : "password";
}

function persistDraft() {
  localStorage.setItem("gdou-draft", JSON.stringify(getPayload(false)));
}

function loadDraft() {
  const raw = localStorage.getItem("gdou-draft");
  if (!raw) {
    setValue("os_name", navigator.platform || "desktop");
    setValue("device_name", navigator.platform || "desktop");
    return;
  }
  try {
    const draft = JSON.parse(raw);
    applyConfig(draft);
  } catch {
    setValue("os_name", navigator.platform || "desktop");
    setValue("device_name", navigator.platform || "desktop");
  }
}

async function invoke(cmd, args = {}) {
  if (state.taskRunning && cmd !== "set_auto_reconnect_cmd") return;
  try {
    state.taskRunning = true;
    setStatus("Working...");
    const invokeFn = window.__TAURI__?.core?.invoke || window.__TAURI__?.invoke || window.tauri?.invoke;
    if (!invokeFn) throw new Error("Tauri bridge unavailable");
    const payload = getPayload();
    const result = await invokeFn(cmd, {
      config: payload,
      ...payload,
      ...args,
    });
    applyResponse(result);
  } catch (err) {
    setStatus(String(err?.message || err));
  } finally {
    state.taskRunning = false;
    persistDraft();
  }
}

function applyResponse(result) {
  if (result?.config) {
    applyConfig(result.config);
  }
  if (result?.status) {
    setStatus(result.status);
  }
  if (typeof result?.online === "boolean") {
    setOnline(result.online);
  }
  if (typeof result?.auto_reconnect === "boolean") {
    setChecked("auto_reconnect", result.auto_reconnect);
    setBadge(result.auto_reconnect ? "Watching" : "Idle", result.auto_reconnect);
  }
}

function setStatus(text) {
  document.getElementById("status_text").textContent = text;
}

function setOnline(online) {
  const el = document.getElementById("online_state");
  el.textContent = online ? "Online" : "Offline";
  el.className = `pill ${online ? "online" : "offline"}`;
}

function setBadge(text, watching) {
  const el = document.getElementById("badge");
  el.textContent = text;
  el.className = `pill ${watching ? "online" : ""}`.trim();
}

loadDraft();
setStatus("Ready");
setBadge("Idle", false);

(async function boot() {
  try {
    const invokeFn = window.__TAURI__?.core?.invoke || window.__TAURI__?.invoke || window.tauri?.invoke;
    const listenFn = window.__TAURI__?.event?.listen;
    if (listenFn) {
      await listenFn("status", (event) => applyResponse(event.payload));
    }
    if (invokeFn) {
      applyResponse(await invokeFn("load_state_cmd"));
    }
  } catch (err) {
    setStatus(String(err?.message || err));
  }
})();
