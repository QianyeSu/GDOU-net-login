import React, { useEffect, useMemo, useRef, useState } from "react";
import { createRoot } from "react-dom/client";
import {
  Activity,
  AlertTriangle,
  CheckCircle2,
  CircleDashed,
  Eye,
  EyeOff,
  LogIn,
  Power,
  RefreshCw,
  Save,
  SearchCheck,
  Settings2,
  ShieldCheck,
  Wifi,
  WifiOff,
  XCircle,
} from "lucide-react";
import "./styles.css";

const defaultForm = {
  username: "",
  password: "",
  portal_url: "http://10.129.1.1/srun_portal_success?ac_id=17&theme=pro",
  probe_url: "http://connectivitycheck.gstatic.com/generate_204",
  ac_id: "",
  retry_seconds: 30,
  auto_query_acid: true,
  auto_reconnect: true,
  show_password: false,
  os_name: "",
  device_name: "",
  n: 200,
  login_type: 1,
};

const navItems = [
  { id: "home", label: "连接", hint: "登录与重连", icon: Wifi },
  { id: "status", label: "状态", hint: "运行概览", icon: Activity },
];

function getInvoke() {
  return window.__TAURI__?.core?.invoke || window.__TAURI__?.invoke || window.tauri?.invoke;
}

function getListen() {
  return window.__TAURI__?.event?.listen;
}

function formatTime(value) {
  if (!value) return "未发生";
  return new Intl.DateTimeFormat("zh-CN", {
    hour12: false,
    hour: "2-digit",
    minute: "2-digit",
    second: "2-digit",
  }).format(value);
}

function formatReceiptState(state) {
  const labels = {
    idle: "待处理",
    pending: "进行中",
    success: "成功",
    warning: "注意",
    error: "失败",
  };
  return labels[state] || state;
}

function App() {
  const [page, setPage] = useState("home");
  const [taskRunning, setTaskRunning] = useState(false);
  const [statusText, setStatusText] = useState("Ready");
  const [online, setOnline] = useState(null);
  const [badge, setBadge] = useState("Watching");
  const [saveReceipt, setSaveReceipt] = useState({
    state: "idle",
    title: "未保存",
    detail: "尚未写入配置",
    at: null,
  });
  const [loginReceipt, setLoginReceipt] = useState({
    state: "idle",
    title: "未登录",
    detail: "等待发起登录",
    at: null,
  });
  const [networkReceipt, setNetworkReceipt] = useState({
    state: "idle",
    title: "未知",
    detail: "等待检测结果",
    at: null,
  });
  const [form, setForm] = useState(defaultForm);
  const [events, setEvents] = useState([
    { kind: "system", text: "界面已加载", id: "seed" },
  ]);
  const lastCommandRef = useRef("load_state_cmd");

  const summary = useMemo(
    () => ({
      portal: form.portal_url || "-",
      probe: form.probe_url || "-",
      retry: `${form.retry_seconds || 30} 秒`,
      user: form.username || "-",
    }),
    [form],
  );

  const onlineLabel = online === true ? "在线" : online === false ? "离线" : "未知";
  const guardLabel = form.auto_reconnect ? "已开启" : "已关闭";

  const activityTone =
    statusText === "Ready"
      ? "neutral"
      : /saved/i.test(statusText)
        ? "save"
        : /online|login|reconnect/i.test(statusText)
          ? "online"
          : /offline/i.test(statusText)
            ? "offline"
            : "status";

  useEffect(() => {
    const raw = localStorage.getItem("gdou-draft");
    if (raw) {
      try {
        setForm((prev) => ({ ...prev, ...JSON.parse(raw) }));
        setSaveReceipt({
          state: "success",
          title: "草稿已载入",
          detail: "本地配置已恢复",
          at: new Date(),
        });
        pushEvent("system", "已载入本地草稿");
      } catch {
        setForm((prev) => ({
          ...prev,
          os_name: navigator.platform || "desktop",
          device_name: navigator.platform || "desktop",
        }));
      }
    } else {
      setForm((prev) => ({
        ...prev,
        os_name: navigator.platform || "desktop",
        device_name: navigator.platform || "desktop",
      }));
    }
  }, []);

  useEffect(() => {
    localStorage.setItem(
      "gdou-draft",
      JSON.stringify({
        ...form,
        password: "",
      }),
    );
  }, [form]);

  useEffect(() => {
    let mounted = true;
    (async () => {
      try {
        const listen = getListen();
        if (listen) {
          await listen("status", (event) => {
            if (!mounted) return;
            applyResponse(event.payload);
          });
        }
        const invoke = getInvoke();
        if (invoke) {
          applyResponse(await invoke("load_state_cmd"));
        } else {
          setStatusText("预览模式");
          pushEvent("system", "浏览器预览模式，未连接 Tauri 后端");
        }
      } catch (err) {
        const message = String(err?.message || err);
        setStatusText(message);
        pushEvent("error", message);
      }
    })();
    return () => {
      mounted = false;
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  function pushEvent(kind, text) {
    setEvents((prev) => [{ kind, text, id: `${Date.now()}-${Math.random()}` }, ...prev].slice(0, 8));
  }

  function updateField(name, value) {
    setForm((prev) => ({ ...prev, [name]: value }));
  }

  function applyResponse(result) {
    if (result?.config) {
      setForm((prev) => ({ ...prev, ...result.config }));
    }
    if (typeof result?.online === "boolean") {
      setOnline(result.online);
      setNetworkReceipt({
        state: result.online ? "success" : "warning",
        title: result.online ? "在线" : "离线",
        detail: result.online ? "探测地址可达" : "探测地址不可达",
        at: new Date(),
      });
      pushEvent("state", result.online ? "当前已联网" : "当前离线");
    }
    if (typeof result?.auto_reconnect === "boolean") {
      updateField("auto_reconnect", result.auto_reconnect);
      setBadge(result.auto_reconnect ? "Watching" : "Idle");
    }
    if (result?.status) {
      setStatusText(result.status);
      const cmd = lastCommandRef.current;
      const success = !/error|fail|failed|panic/i.test(result.status);
      if (cmd === "save_config_cmd" || /saved/i.test(result.status)) {
        setSaveReceipt({
          state: success ? "success" : "error",
          title: success ? "已保存" : "保存失败",
          detail: result.status,
          at: new Date(),
        });
      } else if (cmd === "login_cmd" || /login|online|reconnect/i.test(result.status)) {
        setLoginReceipt({
          state: success ? "success" : "error",
          title: success ? "登录成功" : "登录失败",
          detail: result.status,
          at: new Date(),
        });
      } else if (cmd === "check_status_cmd" || /online|offline/i.test(result.status)) {
        setNetworkReceipt({
          state: success ? "success" : "warning",
          title: /online/i.test(result.status) ? "在线" : "离线",
          detail: result.status,
          at: new Date(),
        });
      }
      pushEvent(/error/i.test(result.status) ? "error" : "status", result.status);
    }
  }

  async function invoke(cmd, args = {}) {
    if (taskRunning && cmd !== "set_auto_reconnect_cmd") return;
    try {
      setTaskRunning(true);
      const invoke = getInvoke();
      lastCommandRef.current = cmd;

      if (cmd === "save_config_cmd") {
        setSaveReceipt({
          state: "pending",
          title: "保存中",
          detail: "正在写入配置",
          at: new Date(),
        });
        pushEvent("action", "开始保存配置");
      }
      if (cmd === "login_cmd") {
        setLoginReceipt({
          state: "pending",
          title: "登录中",
          detail: "正在提交登录请求",
          at: new Date(),
        });
        pushEvent("action", "发起登录");
      }
      if (cmd === "logout_cmd") {
        setLoginReceipt({
          state: "pending",
          title: "正在断开",
          detail: "正在执行退出动作",
          at: new Date(),
        });
        pushEvent("action", "发起退出");
      }
      if (cmd === "check_status_cmd") {
        setNetworkReceipt({
          state: "pending",
          title: "检测中",
          detail: "正在探测网络连通性",
          at: new Date(),
        });
        pushEvent("action", "发起状态检测");
      }

      if (!invoke) {
        const previewResult = {
          status:
            cmd === "save_config_cmd"
              ? "已保存（预览）"
              : cmd === "login_cmd"
                ? "已登录（预览）"
                : cmd === "logout_cmd"
                  ? "已断开（预览）"
                  : "离线（预览）",
          online: cmd === "login_cmd" ? true : cmd === "logout_cmd" || cmd === "check_status_cmd" ? false : undefined,
          auto_reconnect: form.auto_reconnect,
        };
        applyResponse(previewResult);
        return;
      }

      const result = await invoke(cmd, {
        config: form,
        ...form,
        ...args,
      });
      applyResponse(result);
    } catch (err) {
      const message = String(err?.message || err);
      setStatusText(message);
      if (lastCommandRef.current === "save_config_cmd") {
        setSaveReceipt({
          state: "error",
          title: "保存失败",
          detail: message,
          at: new Date(),
        });
      }
      if (lastCommandRef.current === "login_cmd" || lastCommandRef.current === "logout_cmd") {
        setLoginReceipt({
          state: "error",
          title: "登录失败",
          detail: message,
          at: new Date(),
        });
      }
      if (lastCommandRef.current === "check_status_cmd") {
        setNetworkReceipt({
          state: "error",
          title: "检测失败",
          detail: message,
          at: new Date(),
        });
      }
      pushEvent("error", message);
    } finally {
      setTaskRunning(false);
      lastCommandRef.current = "idle";
    }
  }

  return (
    <div className="wrap">
      <div className="window">
        <aside className="sidebar">
          <div className="brand">
            <div className="brand-row">
              <div>
                <h1>GDOU Net Login</h1>
                <p>校园网登录与自动重连</p>
              </div>
              <span className={`dot ${online === true ? "online" : online === false ? "offline" : "idle"}`} />
            </div>
          </div>

          <div className="nav">
            {navItems.map((item) => (
              <NavButton
                key={item.id}
                active={page === item.id}
                icon={item.icon}
                label={item.label}
                hint={item.hint}
                onClick={() => setPage(item.id)}
              />
            ))}
          </div>

          <div className="sidebar-footer">
            <div className="mini-card">
              <span className="mini-label">自动重连</span>
              <span className={`pill ${badge === "Watching" ? "watch" : ""}`}>{badge}</span>
            </div>
            <div className="mini-card">
              <span className="mini-label">连接状态</span>
              <span className={`pill ${online === true ? "online" : online === false ? "offline" : ""}`}>
                {online === true ? "Online" : online === false ? "Offline" : "Unknown"}
              </span>
            </div>
          </div>
        </aside>

        <main className="main">
          <div className="topbar">
            <div>
              <h2>{page === "home" ? "连接" : "状态"}</h2>
              <div className="crumb">
                {page === "home" ? "账号、密码与自动重连" : "运行摘要与最近动作"}
              </div>
            </div>
            <div className="topbar-badges">
              <span className="pill">{currentBadge(summary.portal)}</span>
              <span className={`chip ${activityTone}`}>{statusText}</span>
            </div>
          </div>

          <div className="content">
            {page === "home" ? (
              <section key="home" className="page active desktop-grid">
                <div className="login-stack stack">
                  <div className="hero-card">
                    <div className="hero-copy">
                      <div className="eyebrow">校园网登录器</div>
                      <h3>直接输入账号和密码，先完成登录，再让它在后台守着。</h3>
                      <p>
                        自动重连默认开启，断网后会按设置间隔持续检测并重新登录。
                      </p>
                    </div>
                    <div className="hero-state">
                      <div className={`state-light ${online === true ? "online" : online === false ? "offline" : "idle"}`} />
                      <div>
                        <div className="state-label">当前网络</div>
                        <div className="state-value">{onlineLabel}</div>
                      </div>
                    </div>
                  </div>

                  <div className="control-strip">
                    <StatusTile
                      icon={online === false ? WifiOff : Wifi}
                      label="网络状态"
                      value={onlineLabel}
                      tone={online === true ? "online" : online === false ? "offline" : "idle"}
                    />
                    <StatusTile
                      icon={ShieldCheck}
                      label="自动重连"
                      value={guardLabel}
                      tone={form.auto_reconnect ? "watch" : "idle"}
                    />
                    <StatusTile
                      icon={RefreshCw}
                      label="重试间隔"
                      value={`${form.retry_seconds || 30} 秒`}
                      tone="idle"
                    />
                  </div>

                  <div className="panel-section">
                    <div className="panel-head">
                      <h3>登录信息</h3>
                      <div className="note">先填最常用的字段</div>
                    </div>
                    <div className="panel-body">
                      <div className="grid two-col">
                        <Field label="账号">
                          <input value={form.username} onChange={(e) => updateField("username", e.target.value)} />
                        </Field>
                        <Field label="密码">
                          <input
                            type={form.show_password ? "text" : "password"}
                            value={form.password}
                            onChange={(e) => updateField("password", e.target.value)}
                          />
                        </Field>
                      </div>
                      <div className="checks compact">
                        <label>
                          <input
                            type="checkbox"
                            checked={form.auto_reconnect}
                            onChange={(e) => updateField("auto_reconnect", e.target.checked)}
                          />
                          自动重连
                        </label>
                        <label>
                          <input
                            type="checkbox"
                            checked={form.show_password}
                            onChange={(e) => updateField("show_password", e.target.checked)}
                          />
                          {form.show_password ? <EyeOff size={14} /> : <Eye size={14} />}
                          显示密码
                        </label>
                        <label>
                          <input
                            type="checkbox"
                            checked={form.auto_query_acid}
                            onChange={(e) => updateField("auto_query_acid", e.target.checked)}
                          />
                          自动获取 ac_id
                        </label>
                      </div>
                    </div>
                  </div>

                  <details className="advanced">
                    <summary><Settings2 size={15} /> 高级设置</summary>
                    <div className="panel-body advanced-body">
                      <div className="grid two-col">
                        <Field label="Portal 地址">
                          <input value={form.portal_url} onChange={(e) => updateField("portal_url", e.target.value)} />
                        </Field>
                        <Field label="探测地址">
                          <input value={form.probe_url} onChange={(e) => updateField("probe_url", e.target.value)} />
                        </Field>
                        <Field label="重试间隔(秒)">
                          <input
                            type="number"
                            min="5"
                            max="3600"
                            value={form.retry_seconds}
                            onChange={(e) => updateField("retry_seconds", Number(e.target.value || 30))}
                          />
                        </Field>
                        <Field label="ac_id">
                          <input value={form.ac_id} onChange={(e) => updateField("ac_id", e.target.value)} />
                        </Field>
                        <Field label="OS 名称">
                          <input value={form.os_name} onChange={(e) => updateField("os_name", e.target.value)} />
                        </Field>
                        <Field label="设备名称">
                          <input value={form.device_name} onChange={(e) => updateField("device_name", e.target.value)} />
                        </Field>
                      </div>
                    </div>
                  </details>

                  <div className="actions">
                    <button className="action primary" disabled={taskRunning} onClick={() => invoke("login_cmd")}>
                      <LogIn size={15} />
                      {taskRunning && lastCommandRef.current === "login_cmd" ? "登录中" : "登录"}
                    </button>
                    <button className="action" disabled={taskRunning} onClick={() => invoke("save_config_cmd")}>
                      <Save size={15} />
                      {taskRunning && lastCommandRef.current === "save_config_cmd" ? "保存中" : "保存"}
                    </button>
                    <button className="action soft" disabled={taskRunning} onClick={() => invoke("check_status_cmd")}>
                      <SearchCheck size={15} />
                      {taskRunning && lastCommandRef.current === "check_status_cmd" ? "检测中" : "检测"}
                    </button>
                    <button className="action danger" disabled={taskRunning} onClick={() => invoke("logout_cmd")}>
                      <Power size={15} />
                      {taskRunning && lastCommandRef.current === "logout_cmd" ? "断开中" : "断开"}
                    </button>
                  </div>
                </div>

                <div className="feedback-column">
                  <div className="panel receipt-panel">
                    <div className="panel-head">
                      <h3>操作回执</h3>
                      <div className="note">保存 / 登录 / 网络</div>
                    </div>
                    <div className="panel-body">
                      <div className="receipt-summary">
                        <div className="receipt-summary-item">
                          <span className="receipt-summary-label">当前状态</span>
                          <span className={`receipt-summary-value ${online === true ? "online" : online === false ? "offline" : ""}`}>
                            {onlineLabel}
                          </span>
                        </div>
                        <div className="receipt-summary-item">
                          <span className="receipt-summary-label">守护</span>
                          <span className="receipt-summary-value">{guardLabel}</span>
                        </div>
                      </div>
                      <div className="receipt-grid">
                        <ReceiptCard
                          label="保存回执"
                          receipt={saveReceipt}
                          accent="save"
                        />
                        <ReceiptCard
                          label="登录回执"
                          receipt={loginReceipt}
                          accent="login"
                        />
                        <ReceiptCard
                          label="网络检测"
                          receipt={networkReceipt}
                          accent={online === true ? "online" : online === false ? "offline" : "neutral"}
                        />
                        <div className="watch-card">
                          <div className="watch-head">
                            <span className="watch-label">守护状态</span>
                            <span className={`pill ${badge === "Watching" ? "watch" : ""}`}>{badge}</span>
                          </div>
                          <div className="watch-body">
                            自动重连{guardLabel}，后台会按间隔继续检测。
                          </div>
                        </div>
                      </div>
                    </div>
                  </div>

                  <div className="panel">
                    <div className="panel-head">
                      <h3>最近动作</h3>
                      <div className="note">最近 8 条</div>
                    </div>
                    <div className="panel-body compact">
                      <div className="event-list">
                        {events.map((item) => (
                          <div key={item.id || item.text} className={`event-row ${item.kind}`}>
                            <span className="event-dot" />
                            <span className="event-text">{item.text}</span>
                          </div>
                        ))}
                      </div>
                    </div>
                  </div>
                </div>
              </section>
            ) : (
              <section key="status" className="page active">
                <div className="panel">
                  <div className="panel-head">
                    <h3>运行摘要</h3>
                    <div className="note">当前会话概览</div>
                  </div>
                  <div className="panel-body">
                    <div className="summary">
                      <Row label="在线状态" value={online === true ? "Online" : online === false ? "Offline" : "Unknown"} />
                      <Row label="自动重连" value={badge} />
                      <Row label="Portal" value={summary.portal} />
                      <Row label="探测地址" value={summary.probe} />
                      <Row label="重试间隔" value={summary.retry} />
                      <Row label="账号" value={summary.user} />
                    </div>
                  </div>
                </div>
              </section>
            )}
          </div>

          <div className="status">{statusText}</div>
        </main>
      </div>
    </div>
  );
}

function NavButton({ active, icon: Icon, label, hint, onClick }) {
  return (
    <button className={active ? "active" : ""} onClick={onClick}>
      <span className="nav-icon" aria-hidden="true">
        <Icon size={15} />
      </span>
      <span className="nav-copy">
        <span className="nav-label">{label}</span>
        <span className="nav-hint">{hint}</span>
      </span>
    </button>
  );
}

function StatusTile({ icon: Icon, label, value, tone }) {
  return (
    <div className={`status-tile ${tone}`}>
      <span className="status-tile-icon" aria-hidden="true">
        <Icon size={15} />
      </span>
      <span className="status-tile-copy">
        <span className="status-tile-label">{label}</span>
        <span className="status-tile-value">{value}</span>
      </span>
    </div>
  );
}

function currentBadge(portal) {
  if (!portal) return "Portal";
  try {
    const url = new URL(portal);
    return url.host || portal;
  } catch {
    return portal;
  }
}

function Field({ label, children }) {
  return (
    <div className="field">
      <label>{label}</label>
      {children}
    </div>
  );
}

function Row({ label, value }) {
  return (
    <div className="summary-row">
      <div className="label">{label}</div>
      <div className="value">{value}</div>
    </div>
  );
}

function ReceiptCard({ label, receipt, accent }) {
  const Icon =
    receipt.state === "success"
      ? CheckCircle2
      : receipt.state === "warning"
        ? AlertTriangle
        : receipt.state === "error"
          ? XCircle
          : receipt.state === "pending"
            ? RefreshCw
            : CircleDashed;

  return (
    <div className={`receipt-card ${accent} ${receipt.state}`}>
      <div className="receipt-head">
        <div className="receipt-title-wrap">
          <span className={`receipt-icon ${receipt.state}`} aria-hidden="true">
            <Icon size={15} />
          </span>
          <div>
            <div className="receipt-label">{label}</div>
            <div className="receipt-title">{receipt.title}</div>
          </div>
        </div>
        <span className={`receipt-pill ${receipt.state}`}>{formatReceiptState(receipt.state)}</span>
      </div>
      <div className="receipt-detail">{receipt.detail}</div>
      <div className="receipt-meta">
        <span>时间</span>
        <strong>{formatTime(receipt.at)}</strong>
      </div>
    </div>
  );
}

createRoot(document.getElementById("root")).render(<App />);
