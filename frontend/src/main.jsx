import React, { useEffect, useMemo, useRef, useState } from "react";
import { createRoot } from "react-dom/client";
import { getVersion as getTauriVersion } from "@tauri-apps/api/app";
import { invoke as tauriInvoke } from "@tauri-apps/api/core";
import { listen as tauriListen } from "@tauri-apps/api/event";
import { relaunch as tauriRelaunch } from "@tauri-apps/plugin-process";
import { check as tauriCheckUpdate } from "@tauri-apps/plugin-updater";
import {
  Activity,
  AlertTriangle,
  Bug,
  CheckCircle2,
  CircleDashed,
  Eye,
  EyeOff,
  Github,
  LogIn,
  Palette,
  Power,
  RefreshCw,
  Save,
  SearchCheck,
  Settings2,
  Wifi,
  WifiOff,
  XCircle,
} from "lucide-react";
import "./styles.css";

const REPOSITORY_URL = "https://github.com/QianyeSu/GDOU-net-login";
const PACKAGE_VERSION = __APP_VERSION__;
const THEME_STORAGE_KEY = "gdou-theme-v2";
const SIDEBAR_WIDTH_STORAGE_KEY = "gdou-sidebar-width";
const EMPTY_NETWORK_MONITOR = {
  loading: false,
  error: "",
  samples: [],
  adapters: [],
  totalDownBps: 0,
  totalUpBps: 0,
  peakBps: 0,
  totalBytes: 0,
  activeAdapters: 0,
  lastUpdated: null,
};
const EMPTY_UPDATE_NOTICE = {
  visible: false,
  update: null,
  source: "manual",
};
const NETWORK_TOTAL_HINT =
  "来自 Windows 本机网卡计数器，可能包含 EasyConnect、Clash TUN、虚拟网卡等重复统计；不代表校园网套餐流量";

const defaultForm = {
  username: "",
  password: "",
  portal_url: "",
  probe_url: "http://www.msftconnecttest.com/connecttest.txt",
  ac_id: "",
  user_ip: "",
  bind_ip: "",
  retry_seconds: 15,
  online_check_seconds: 60,
  auto_query_acid: true,
  auto_reconnect: true,
  accept_terms: true,
  show_password: false,
  os_name: "",
  device_name: "",
  n: 200,
  login_type: 1,
};

const navItems = [
  { id: "home", label: "连接", hint: "登录与重连", icon: Wifi },
  { id: "status", label: "状态", hint: "运行概览", icon: Activity },
  { id: "settings", label: "设置", hint: "主题与偏好", icon: Settings2 },
];

const themes = [
  {
    id: "skyborn",
    label: "Skyborn 浅蓝",
    detail: "低饱和浅蓝风格",
  },
  {
    id: "default",
    label: "默认白色",
    detail: "清爽白色界面",
  },
  {
    id: "dark",
    label: "暗色模式",
    detail: "适合夜间和远程桌面",
  },
];

function getInvoke() {
  if (window.__TAURI_INTERNALS__) return tauriInvoke;
  return window.__TAURI__?.core?.invoke || window.__TAURI__?.invoke || window.tauri?.invoke;
}

function getListen() {
  if (window.__TAURI_INTERNALS__) return tauriListen;
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

function formatEventTime(item) {
  const timestamp = Number(String(item?.id || "").split("-")[0]);
  return Number.isFinite(timestamp) && timestamp > 0 ? formatMonitorTime(timestamp) : "刚刚";
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

function autoSaveSnapshot(value) {
  return JSON.stringify({
    username: value.username || "",
    password: value.password || "",
    portal_url: value.portal_url || "",
    probe_url: value.probe_url || "",
    ac_id: value.ac_id || "",
    user_ip: value.user_ip || "",
    bind_ip: value.bind_ip || "",
    retry_seconds: Number(value.retry_seconds || 15),
    online_check_seconds: Number(value.online_check_seconds || 60),
    auto_query_acid: Boolean(value.auto_query_acid),
    auto_reconnect: Boolean(value.auto_reconnect),
    os_name: value.os_name || "",
    device_name: value.device_name || "",
    n: Number(value.n || 200),
    login_type: Number(value.login_type || 1),
  });
}

function buildNetworkMonitorState(previousState, previousSnapshot, snapshot) {
  const adapters = Array.isArray(snapshot?.adapters) ? snapshot.adapters : [];
  const previousByName = new Map(
    (previousSnapshot?.adapters || []).map((adapter) => [adapter.name, adapter]),
  );
  const elapsedSeconds = Math.max(
    0.5,
    ((Number(snapshot?.timestamp_ms || 0) - Number(previousSnapshot?.timestamp_ms || 0)) || 1000) /
      1000,
  );

  let totalDownBps = 0;
  let totalUpBps = 0;
  let totalBytes = 0;

  const rows = adapters
    .map((adapter) => {
      const previous = previousByName.get(adapter.name);
      const backendDownDelta = Number(adapter.received_per_refresh || 0);
      const backendUpDelta = Number(adapter.transmitted_per_refresh || 0);
      const downDelta = backendDownDelta || (previous
        ? Math.max(0, Number(adapter.received_bytes || 0) - Number(previous.received_bytes || 0))
        : 0);
      const upDelta = backendUpDelta || (previous
        ? Math.max(0, Number(adapter.transmitted_bytes || 0) - Number(previous.transmitted_bytes || 0))
        : 0);
      const downBps = downDelta / elapsedSeconds;
      const upBps = upDelta / elapsedSeconds;
      const speedBps = downBps + upBps;

      totalDownBps += downBps;
      totalUpBps += upBps;
      totalBytes += Number(adapter.total_bytes || 0);

      return {
        ...adapter,
        downBps,
        upBps,
        speedBps,
      };
    })
    .sort((a, b) => b.speedBps - a.speedBps || Number(b.total_bytes || 0) - Number(a.total_bytes || 0));

  const totalSpeed = totalDownBps + totalUpBps;
  const samples = [...(previousState.samples || []), totalSpeed].slice(-60);
  const peakBps = Math.max(previousState.peakBps || 0, totalSpeed, ...samples);

  return {
    loading: false,
    error: "",
    samples,
    adapters: rows,
    totalDownBps,
    totalUpBps,
    peakBps,
    totalBytes,
    activeAdapters: rows.filter((adapter) => adapter.speedBps > 1024 || adapter.is_active).length,
    lastUpdated: new Date(),
  };
}

function formatBytes(value) {
  const bytes = Number(value || 0);
  const units = ["B", "KB", "MB", "GB", "TB"];
  let size = bytes;
  let index = 0;
  while (size >= 1024 && index < units.length - 1) {
    size /= 1024;
    index += 1;
  }
  const digits = index <= 1 ? 0 : 2;
  return `${size.toFixed(digits)} ${units[index]}`;
}

function formatSpeed(value) {
  return `${formatBytes(value)}/s`;
}

function formatMonitorTime(value) {
  if (!value) return "-";
  return new Intl.DateTimeFormat("zh-CN", {
    hour12: false,
    hour: "2-digit",
    minute: "2-digit",
  }).format(value);
}

function sparklinePaths(samples, width = 520, height = 110) {
  const values = samples?.length ? samples : [0];
  const max = Math.max(...values, 1);
  const points = values.map((value, index) => {
    const x = values.length === 1 ? width : (index / (values.length - 1)) * width;
    const y = height - (value / max) * (height - 14) - 7;
    return { x, y };
  });

  if (points.length === 1) {
    const point = points[0];
    const linePath = `M 0 ${point.y.toFixed(1)} C ${width * 0.32} ${point.y.toFixed(1)}, ${width * 0.68} ${point.y.toFixed(1)}, ${width} ${point.y.toFixed(1)}`;
    return {
      linePath,
      fillPath: `${linePath} L ${width} ${height} L 0 ${height} Z`,
    };
  }

  const linePath = points.reduce((path, point, index) => {
    if (index === 0) return `M ${point.x.toFixed(1)} ${point.y.toFixed(1)}`;
    const previous = points[index - 1];
    const midX = (previous.x + point.x) / 2;
    return `${path} C ${midX.toFixed(1)} ${previous.y.toFixed(1)}, ${midX.toFixed(1)} ${point.y.toFixed(1)}, ${point.x.toFixed(1)} ${point.y.toFixed(1)}`;
  }, "");
  const first = points[0];
  const last = points[points.length - 1];

  return {
    linePath,
    fillPath: `${linePath} L ${last.x.toFixed(1)} ${height} L ${first.x.toFixed(1)} ${height} Z`,
  };
}

function App() {
  const [page, setPage] = useState("home");
  const [theme, setTheme] = useState(() => localStorage.getItem(THEME_STORAGE_KEY) || "skyborn");
  const [sidebarWidth, setSidebarWidth] = useState(() => {
    const saved = Number(localStorage.getItem(SIDEBAR_WIDTH_STORAGE_KEY));
    return Number.isFinite(saved) ? Math.min(340, Math.max(208, saved)) : 228;
  });
  const [resizingSidebar, setResizingSidebar] = useState(false);
  const [taskRunning, setTaskRunning] = useState(false);
  const [statusText, setStatusText] = useState("Ready");
  const [online, setOnline] = useState(null);
  const [badge, setBadge] = useState("Watching");
  const [startupEnabled, setStartupEnabled] = useState(false);
  const [appVersion, setAppVersion] = useState(PACKAGE_VERSION);
  const [updating, setUpdating] = useState(false);
  const [updateNotice, setUpdateNotice] = useState(EMPTY_UPDATE_NOTICE);
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
  const autoSaveReadyRef = useRef(false);
  const autoSaveSnapshotRef = useRef(autoSaveSnapshot(defaultForm));
  const previousNetworkSnapshotRef = useRef(null);
  const resizeStartRef = useRef({ x: 0, width: 228 });
  const startupUpdateCheckedRef = useRef(false);
  const [networkMonitor, setNetworkMonitor] = useState(EMPTY_NETWORK_MONITOR);
  const [networkInterfaces, setNetworkInterfaces] = useState([]);
  const [networkInterfacesLoading, setNetworkInterfacesLoading] = useState(false);
  const [networkInterfacesError, setNetworkInterfacesError] = useState("");
  const [appWindowVisible, setAppWindowVisible] = useState(true);

  const summary = useMemo(
    () => ({
      portal: form.portal_url || "-",
      probe: form.probe_url || "-",
      bindIp: form.bind_ip || "自动选择",
      retry: `${form.retry_seconds || 15} 秒`,
      onlineCheck: `${form.online_check_seconds || 60} 秒`,
      user: form.username || "-",
      version: `v${appVersion}`,
    }),
    [form, appVersion],
  );

  const networkToolHint = /EasyConnect|虚拟网卡|TUN|网络工具|Sangfor|Clash|mihomo/i.test(statusText);
  const reconnectPausedForEasyConnect = /EasyConnect 已连接，校园网自动重连已暂停/.test(statusText);
  const onlineLabel = online === true ? "SRUN 在线" : online === false ? "SRUN 未在线" : "未知";
  const guardLabel = form.auto_reconnect ? "已开启" : "已关闭";
  const guardDisplay = reconnectPausedForEasyConnect ? "已暂停" : form.auto_reconnect ? "守护中" : "未开启";
  const homeHint =
    reconnectPausedForEasyConnect
      ? "EasyConnect 已连接，校园网自动重连已暂停"
      : online === true
      ? "当前 SRUN 账号在线，后台会按巡检间隔轻量检查"
      : online === false
        ? networkToolHint
          ? "SRUN 未在线，但网络可能正由 EasyConnect 或虚拟网卡接管；需要校园网账号在线时请重新登录"
          : "检测到 SRUN 未在线，可以手动登录；开启自动重连后会按重试间隔继续尝试"
        : "首次使用先填写账号密码并登录，必要时再打开高级设置自动探测";
  const pageTitle = page === "home" ? "连接" : page === "status" ? "状态" : "设置";
  const pageCrumb =
    page === "home"
      ? "账号、密码与自动重连"
      : page === "status"
        ? "运行摘要"
        : "主题、偏好与高级网络";

  const activityTone =
    reconnectPausedForEasyConnect
      ? "status"
      : compactStatus(statusText) === "Ready"
      ? "neutral"
      : /saved|已保存/i.test(statusText)
        ? "save"
        : /online|login|reconnect|在线|登录|重连/i.test(statusText)
          ? "online"
          : /offline|离线/i.test(statusText)
            ? "offline"
            : "status";

  useEffect(() => {
    localStorage.removeItem("gdou-draft");
    setForm((prev) => ({
      ...prev,
      os_name: navigator.platform || "desktop",
      device_name: navigator.platform || "desktop",
    }));
  }, []);

  useEffect(() => {
    const snapshot = autoSaveSnapshot(form);
    if (!autoSaveReadyRef.current) {
      autoSaveSnapshotRef.current = snapshot;
      return;
    }
    if (snapshot === autoSaveSnapshotRef.current) return;

    const invoke = getInvoke();
    if (!invoke) return;
    setSaveReceipt({
      state: "pending",
      title: "自动保存中",
      detail: "正在写入配置",
      at: new Date(),
    });

    const timer = window.setTimeout(async () => {
      const requestForm = { ...form, accept_terms: true };
      try {
        const result = await invoke("autosave_config_cmd", {
          config: requestForm,
          ...requestForm,
        });
        setSaveReceipt({
          state: "success",
          title: "已自动保存",
          detail: form.password ? "已保存账号、密码和设置" : result?.status || "Saved",
          at: new Date(),
        });
        autoSaveSnapshotRef.current = snapshot;
      } catch (err) {
        setSaveReceipt({
          state: "error",
          title: "自动保存失败",
          detail: String(err?.message || err),
          at: new Date(),
        });
      }
    }, 1000);

    return () => window.clearTimeout(timer);
  }, [form]);

  useEffect(() => {
    localStorage.setItem(THEME_STORAGE_KEY, theme);
  }, [theme]);

  useEffect(() => {
    localStorage.setItem(SIDEBAR_WIDTH_STORAGE_KEY, String(sidebarWidth));
  }, [sidebarWidth]);

  useEffect(() => {
    if (page !== "home") return undefined;

    let stopped = false;
    let timer = null;

    async function refreshNetworkMonitor() {
      if (stopped || !appWindowVisible || document.visibilityState !== "visible") return;
      const invoke = getInvoke();
      if (!invoke) {
        setNetworkMonitor((prev) => ({
          ...prev,
          error: "预览模式未连接后端",
          loading: false,
        }));
        return;
      }

      try {
        setNetworkMonitor((prev) => ({ ...prev, loading: true, error: "" }));
        const snapshot = await invoke("network_monitor_snapshot_cmd");
        setNetworkMonitor((prev) =>
          buildNetworkMonitorState(prev, previousNetworkSnapshotRef.current, snapshot),
        );
        previousNetworkSnapshotRef.current = snapshot;
      } catch (err) {
        setNetworkMonitor((prev) => ({
          ...prev,
          loading: false,
          error: String(err?.message || err),
        }));
      }
    }

    function stopTimer() {
      if (timer) {
        window.clearInterval(timer);
        timer = null;
      }
    }

    function startTimer() {
      if (timer || stopped || !appWindowVisible || document.visibilityState !== "visible") return;
      refreshNetworkMonitor();
      timer = window.setInterval(refreshNetworkMonitor, 1000);
    }

    function handleVisibilityChange() {
      if (document.visibilityState === "visible") {
        startTimer();
      } else {
        stopTimer();
      }
    }

    startTimer();
    document.addEventListener("visibilitychange", handleVisibilityChange);
    return () => {
      stopped = true;
      stopTimer();
      document.removeEventListener("visibilitychange", handleVisibilityChange);
    };
  }, [page, appWindowVisible]);

  useEffect(() => {
    let mounted = true;
    getTauriVersion()
      .then((version) => {
        if (mounted && version) setAppVersion(version);
      })
      .catch(() => {
        if (mounted) setAppVersion(PACKAGE_VERSION);
      });
    return () => {
      mounted = false;
    };
  }, []);

  useEffect(() => {
    const invoke = getInvoke();
    if (!invoke || startupUpdateCheckedRef.current) return undefined;
    startupUpdateCheckedRef.current = true;

    const timer = window.setTimeout(() => {
      runUpdateCheck({ silent: true, source: "startup" });
    }, 4000);

    return () => window.clearTimeout(timer);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  useEffect(() => {
    if (!resizingSidebar) return;

    function handlePointerMove(event) {
      const delta = event.clientX - resizeStartRef.current.x;
      setSidebarWidth(Math.min(340, Math.max(208, resizeStartRef.current.width + delta)));
    }

    function handlePointerUp() {
      setResizingSidebar(false);
      document.body.style.cursor = "";
      document.body.style.userSelect = "";
    }

    document.body.style.cursor = "col-resize";
    document.body.style.userSelect = "none";
    window.addEventListener("pointermove", handlePointerMove);
    window.addEventListener("pointerup", handlePointerUp);
    return () => {
      window.removeEventListener("pointermove", handlePointerMove);
      window.removeEventListener("pointerup", handlePointerUp);
      document.body.style.cursor = "";
      document.body.style.userSelect = "";
    };
  }, [resizingSidebar]);

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
          await listen("window-visibility", (event) => {
            if (!mounted) return;
            setAppWindowVisible(Boolean(event.payload));
          });
        }
        const invoke = getInvoke();
        if (invoke) {
          const initialState = await invoke("load_state_cmd");
          if (initialState?.config) {
            autoSaveSnapshotRef.current = autoSaveSnapshot({
              ...defaultForm,
              ...initialState.config,
              accept_terms: true,
            });
            setSaveReceipt({
              state: initialState.config.username || initialState.config.password ? "success" : "idle",
              title: initialState.config.username || initialState.config.password ? "已加载保存内容" : "未保存",
              detail:
                initialState.config.username || initialState.config.password
                  ? "已从本机读取账号配置"
                  : "尚未写入配置",
              at: initialState.config.username || initialState.config.password ? new Date() : null,
            });
          }
          applyResponse(initialState);
          autoSaveReadyRef.current = true;
        } else {
          setStatusText("预览模式");
          pushEvent("system", "浏览器预览模式，未连接 Tauri 后端");
          autoSaveReadyRef.current = true;
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
    setEvents((prev) => [{ kind, text: compactEventText(text), id: `${Date.now()}-${Math.random()}` }, ...prev].slice(0, 8));
  }

  function updateField(name, value) {
    setForm((prev) => ({ ...prev, [name]: value }));
  }

  async function refreshNetworkInterfaces({ force = true } = {}) {
    const invoke = getInvoke();
    if (!invoke) {
      setNetworkInterfacesError("预览模式未连接后端");
      return;
    }
    setNetworkInterfacesLoading(true);
    setNetworkInterfacesError("");
    try {
      const items = await invoke("list_network_interfaces_cmd", { force });
      setNetworkInterfaces(Array.isArray(items) ? items : []);
    } catch (err) {
      setNetworkInterfacesError(String(err?.message || err));
    } finally {
      setNetworkInterfacesLoading(false);
    }
  }

  function applyResponse(result) {
    if (result?.config) {
      setForm((prev) => {
        const next = { ...prev, ...result.config, accept_terms: true };
        if (!result.config.password && prev.password) {
          next.password = prev.password;
        }
        return next;
      });
    }
    if (typeof result?.online === "boolean") {
      setOnline(result.online);
      setNetworkReceipt({
        state: result.online ? "success" : "warning",
        title: result.online ? "SRUN 在线" : "SRUN 未在线",
        detail: result.online ? "校园网账号在线" : "校园网账号未在线或状态接口不可达",
        at: new Date(),
      });
      pushEvent("state", result.online ? "SRUN 当前在线" : "SRUN 当前未在线");
    }
    if (typeof result?.auto_reconnect === "boolean") {
      updateField("auto_reconnect", result.auto_reconnect);
      setBadge(result.auto_reconnect ? "Watching" : "Idle");
    }
    if (typeof result?.startup_enabled === "boolean") {
      setStartupEnabled(result.startup_enabled);
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
      } else if (cmd === "diagnose_cmd") {
        setNetworkReceipt({
          state: success ? "success" : "error",
          title: success ? "诊断完成" : "诊断失败",
          detail: result.status,
          at: new Date(),
        });
      } else if (cmd === "reconnect_self_test_cmd") {
        setNetworkReceipt({
          state: success ? "success" : "error",
          title: success ? "自测完成" : "自测失败",
          detail: result.status,
          at: new Date(),
        });
      } else if (cmd === "logout_cmd") {
        const stillOnline = result.online === true;
        setLoginReceipt({
          state: success && !stillOnline ? "success" : "warning",
          title: success && !stillOnline ? "已断开" : "需要确认",
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
          title: /online/i.test(result.status) ? "SRUN 在线" : "SRUN 未在线",
          detail: result.status,
          at: new Date(),
        });
      } else if (cmd === "detect_portal_cmd") {
        setNetworkReceipt({
          state: success ? "success" : "error",
          title: success ? "探测成功" : "探测失败",
          detail: result.status,
          at: new Date(),
        });
      }
      pushEvent(/error/i.test(result.status) ? "error" : "status", result.status);
    }
  }

  async function invoke(cmd, args = {}) {
    if (taskRunning && cmd !== "set_auto_reconnect_cmd" && cmd !== "set_startup_enabled_cmd") return;
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
      if (cmd === "detect_portal_cmd") {
        setNetworkReceipt({
          state: "pending",
          title: "探测中",
          detail: "正在识别校园网认证地址",
          at: new Date(),
        });
        pushEvent("action", "自动探测 Portal");
      }
      if (cmd === "diagnose_cmd") {
        setNetworkReceipt({
          state: "pending",
          title: "诊断中",
          detail: "正在检查 Portal、ac_id、在线状态和探测链路",
          at: new Date(),
        });
        pushEvent("action", "启动诊断");
      }
      if (cmd === "reconnect_self_test_cmd") {
        setNetworkReceipt({
          state: "pending",
          title: "自测中",
          detail: "正在执行退出、重新登录和状态检测",
          at: new Date(),
        });
        pushEvent("action", "启动重连自测");
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
                  : cmd === "detect_portal_cmd"
                    ? "已探测 Portal（预览）"
                    : cmd === "diagnose_cmd"
                      ? "诊断完成（预览）"
                      : cmd === "reconnect_self_test_cmd"
                        ? "重连自测完成（预览）"
                        : cmd === "set_startup_enabled_cmd"
                      ? args.enabled ? "已开启开机启动（预览）" : "已关闭开机启动（预览）"
                      : "离线（预览）",
          online: cmd === "login_cmd" ? true : cmd === "logout_cmd" || cmd === "check_status_cmd" ? false : undefined,
          auto_reconnect: form.auto_reconnect,
          startup_enabled: cmd === "set_startup_enabled_cmd" ? args.enabled : startupEnabled,
        };
        applyResponse(previewResult);
        return;
      }

      const requestForm = { ...form, accept_terms: true };
      const result = await invoke(cmd, {
        config: requestForm,
        ...requestForm,
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
      if (lastCommandRef.current === "check_status_cmd" || lastCommandRef.current === "diagnose_cmd" || lastCommandRef.current === "reconnect_self_test_cmd") {
        setNetworkReceipt({
          state: "error",
          title: "检测失败",
          detail: message,
          at: new Date(),
        });
      }
      if (lastCommandRef.current === "detect_portal_cmd") {
        setNetworkReceipt({
          state: "error",
          title: "探测失败",
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

  async function openRepository() {
    const invoke = getInvoke();
    if (!invoke) {
      window.open(REPOSITORY_URL, "_blank", "noopener,noreferrer");
      return;
    }
    try {
      await invoke("open_repository_cmd");
      pushEvent("system", "已打开 GitHub 仓库");
    } catch (err) {
      const message = String(err?.message || err);
      setStatusText(message);
      pushEvent("error", message);
    }
  }

  async function runUpdateCheck({ silent = false, source = "manual" } = {}) {
    const invoke = getInvoke();
    if (!invoke) {
      if (!silent) {
        window.open(`${REPOSITORY_URL}/releases`, "_blank", "noopener,noreferrer");
      }
      return;
    }
    try {
      if (!silent) {
        setUpdating(true);
        setStatusText("正在检查更新...");
      }
      const update = await tauriCheckUpdate({ timeout: 15000 });
      if (!update) {
        if (!silent) {
          setStatusText(`当前已是最新版本 v${appVersion}`);
        }
        return;
      }

      setUpdateNotice({ visible: true, update, source });
      if (!silent) {
        setStatusText(`发现新版本 v${update.version}`);
      }
    } catch (err) {
      const message = String(err?.message || err);
      if (silent) {
        return;
      } else {
        const fallback = `${message}；已打开更新页面供手动下载`;
        setStatusText(fallback);
        try {
          await invoke("open_releases_cmd");
        } catch {
          window.open(`${REPOSITORY_URL}/releases`, "_blank", "noopener,noreferrer");
        }
      }
    } finally {
      if (!silent) {
        setUpdating(false);
      }
    }
  }

  async function checkUpdates() {
    await runUpdateCheck({ silent: false, source: "manual" });
  }

  async function installUpdate(update) {
    if (!update) return;
    try {
      setUpdating(true);
      setUpdateNotice((prev) => ({ ...prev, visible: false }));
      setStatusText(`正在下载并安装 v${update.version}...`);
      pushEvent("system", `开始安装 v${update.version}`);
      await update.downloadAndInstall((event) => {
        if (event.event === "Started") {
          setStatusText(`开始下载 v${update.version}`);
        } else if (event.event === "Finished") {
          setStatusText(`v${update.version} 安装完成，正在重启`);
        }
      });
      await tauriRelaunch();
    } catch (err) {
      const message = String(err?.message || err);
      setStatusText(`更新失败：${message}`);
      pushEvent("error", `更新失败：${message}`);
      try {
        const invoke = getInvoke();
        await invoke?.("open_releases_cmd");
      } catch {
        window.open(`${REPOSITORY_URL}/releases`, "_blank", "noopener,noreferrer");
      }
    } finally {
      setUpdating(false);
    }
  }

  return (
    <div className="wrap" data-theme={theme}>
      <div className={`window ${resizingSidebar ? "is-resizing" : ""}`} style={{ "--sidebar-width": `${sidebarWidth}px` }}>
        <aside className="sidebar">
          <div className="brand">
            <div className="brand-row">
              <div>
                <h1>GDOU Net Login</h1>
                <p>广东海洋大学校园网助手</p>
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

          <div className="sidebar-activity">
            <div className="sidebar-section-head">
              <span>最近动作</span>
              <small>最新 {Math.min(events.length, 5)} 条</small>
            </div>
            <div className="event-list sidebar-events">
              {events.slice(0, 5).map((item) => (
                <div key={item.id || item.text} className={`event-row ${item.kind}`}>
                  <span className="event-dot" />
                  <span className="event-text">{item.text}</span>
                </div>
              ))}
            </div>
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
            <div className="version-line">v{appVersion}</div>
          </div>
        </aside>

        <div
          className="sidebar-resizer"
          role="separator"
          aria-label="调整侧边栏宽度"
          aria-orientation="vertical"
          tabIndex={0}
          onPointerDown={(event) => {
            resizeStartRef.current = { x: event.clientX, width: sidebarWidth };
            setResizingSidebar(true);
          }}
          onDoubleClick={() => setSidebarWidth(228)}
        />

        <main className="main">
          <div className="topbar">
            <div>
              <h2>{pageTitle}</h2>
              <div className="crumb">{pageCrumb}</div>
            </div>
            {page !== "home" ? (
              <div className="topbar-badges">
                <span className="pill">{currentBadge(summary.portal)}</span>
                <span className={`chip ${activityTone}`} title={statusText}>{compactStatus(statusText)}</span>
              </div>
            ) : null}
          </div>

          <div className={`content ${page === "home" ? "home-content" : ""}`}>
            {page === "home" ? (
              <section key="home" className="page active home-dashboard">
                <div className="network-hero-card">
                  <NetworkMonitorPanel
                    monitor={networkMonitor}
                    compact
                    online={online}
                    onlineLabel={onlineLabel}
                    currentIp={form.user_ip || ""}
                  />
                </div>

                <div className="home-card auth-card">
                  <div className="home-card-head">
                    <div>
                      <div className="eyebrow">一键认证</div>
                      <h3>{online === true ? "当前已连接" : "登录校园网"}</h3>
                    </div>
                    <span className={`home-mini-status ${online === true ? "online" : online === false ? "offline" : "idle"}`}>
                      <span aria-hidden="true" />
                      {online === true ? "已连接" : online === false ? "未连接" : "待检测"}
                    </span>
                  </div>

                  <div className="auth-fields">
                    <Field label="账号">
                      <input
                        className="auth-input"
                        value={form.username}
                        autoComplete="username"
                        placeholder="输入校园网账号"
                        onChange={(e) => updateField("username", e.target.value)}
                      />
                    </Field>
                    <Field label="密码">
                      <div className="password-field">
                        <input
                          className="auth-input"
                          type={form.show_password ? "text" : "password"}
                          value={form.password}
                          autoComplete="current-password"
                          placeholder="输入密码"
                          onChange={(e) => updateField("password", e.target.value)}
                        />
                        <button
                          className="icon-toggle"
                          type="button"
                          aria-label={form.show_password ? "隐藏密码" : "显示密码"}
                          onClick={() => updateField("show_password", !form.show_password)}
                        >
                          {form.show_password ? <EyeOff size={17} /> : <Eye size={17} />}
                        </button>
                      </div>
                    </Field>
                  </div>

                  <label className="auth-check">
                    <input
                      type="checkbox"
                      checked={form.auto_reconnect}
                      onChange={(e) => updateField("auto_reconnect", e.target.checked)}
                    />
                    <span>保持后台自动守护</span>
                  </label>

                  <div className={`auth-cta-row ${online === true ? "connected" : ""}`}>
                    <button
                      className="action primary auth-primary"
                      disabled={taskRunning || online === true}
                      onClick={() => invoke("login_cmd")}
                    >
                      <LogIn size={18} />
                      {taskRunning && lastCommandRef.current === "login_cmd" ? "登录中" : online === true ? "已连接" : "一键登录"}
                    </button>
                    {online === true ? (
                      <button className="action danger auth-danger" disabled={taskRunning} onClick={() => invoke("logout_cmd")}>
                        <Power size={17} />
                        {taskRunning && lastCommandRef.current === "logout_cmd" ? "断开中" : "断开连接"}
                      </button>
                    ) : null}
                  </div>

                  <div className="auth-secondary-actions">
                    <button className="action soft" disabled={taskRunning} onClick={() => invoke("save_config_cmd")}>
                      <Save size={15} />
                      {taskRunning && lastCommandRef.current === "save_config_cmd" ? "保存中" : "保存"}
                    </button>
                    <button className="action soft" disabled={taskRunning} onClick={() => invoke("check_status_cmd")}>
                      <SearchCheck size={15} />
                      {taskRunning && lastCommandRef.current === "check_status_cmd" ? "检测中" : "检测"}
                    </button>
                  </div>
                </div>

                <div className="home-card timeline-card">
                  <div className="home-card-head">
                    <div>
                      <div className="eyebrow">状态日志</div>
                      <h3>最近动态</h3>
                    </div>
                    <span className="timeline-count">{events.length ? Math.min(events.length, 3) : 0}</span>
                  </div>
                  <div className="status-timeline" aria-live="polite">
                    {events.slice(0, 3).map((item) => (
                      <div className={`timeline-item ${item.kind}`} key={item.id}>
                        <span className="timeline-dot" aria-hidden="true" />
                        <div>
                          <time>{formatEventTime(item)}</time>
                          <p>{item.text}</p>
                        </div>
                      </div>
                    ))}
                    {!events.length ? (
                      <div className="timeline-empty">暂无状态记录</div>
                    ) : null}
                  </div>
                  <div className="timeline-actions">
                    <button className="action soft compact" disabled={taskRunning} onClick={() => invoke("reconnect_self_test_cmd")}>
                      <RefreshCw size={14} />
                      {taskRunning && lastCommandRef.current === "reconnect_self_test_cmd" ? "自测中" : "重连自测"}
                    </button>
                  </div>
                </div>
              </section>
            ) : page === "status" ? (
              <section key="status" className="page active status-page">
                <div className="panel">
                  <div className="panel-head">
                    <h3>运行摘要</h3>
                    <div className="note">只保留关键状态</div>
                  </div>
                  <div className="panel-body">
                    <div className="status-brief-grid">
                      <StatusTile
                        icon={online === true ? Wifi : WifiOff}
                        label="校园网"
                        value={online === true ? "已连接" : online === false ? "未连接" : "待检测"}
                        tone={online === true ? "online" : online === false ? "offline" : "neutral"}
                      />
                      <StatusTile
                        icon={RefreshCw}
                        label="自动守护"
                        value={guardDisplay}
                        tone={form.auto_reconnect ? "watch" : "neutral"}
                      />
                      <StatusTile icon={Save} label="配置保存" value={saveReceipt.title} tone="save" />
                    </div>
                    <div className="summary status-brief-summary">
                      <Row label="重试间隔" value={summary.retry} />
                      <Row label="在线巡检" value={summary.onlineCheck} />
                      <Row label="软件版本" value={summary.version} />
                    </div>
                  </div>
                </div>

                <div className="panel">
                  <div className="panel-head">
                    <h3>最近动态</h3>
                    <div className="note">最新 {Math.min(events.length, 5)} 条</div>
                  </div>
                  <div className="panel-body">
                    <div className="status-timeline status-page-timeline" aria-live="polite">
                      {events.slice(0, 5).map((item) => (
                        <div className={`timeline-item ${item.kind}`} key={item.id}>
                          <span className="timeline-dot" aria-hidden="true" />
                          <div>
                            <time>{formatEventTime(item)}</time>
                            <p>{item.text}</p>
                          </div>
                        </div>
                      ))}
                      {!events.length ? <div className="timeline-empty">暂无状态记录</div> : null}
                    </div>
                    <div className="advanced-actions status-actions">
                      <button className="action soft" disabled={taskRunning} onClick={() => invoke("detect_portal_cmd")}>
                        <SearchCheck size={15} />
                        {taskRunning && lastCommandRef.current === "detect_portal_cmd" ? "探测中" : "自动探测 Portal"}
                      </button>
                      <button className="action soft" disabled={taskRunning} onClick={() => invoke("diagnose_cmd")}>
                        <Bug size={15} />
                        {taskRunning && lastCommandRef.current === "diagnose_cmd" ? "诊断中" : "诊断"}
                      </button>
                      <button className="action soft" disabled={taskRunning} onClick={() => invoke("check_status_cmd")}>
                        <SearchCheck size={15} />
                        {taskRunning && lastCommandRef.current === "check_status_cmd" ? "检测中" : "检测"}
                      </button>
                      <button className="action soft" disabled={taskRunning} onClick={() => invoke("reconnect_self_test_cmd")}>
                        <RefreshCw size={15} />
                        {taskRunning && lastCommandRef.current === "reconnect_self_test_cmd" ? "自测中" : "重连自测"}
                      </button>
                    </div>
                  </div>
                </div>

                <div className="panel">
                  <div className="panel-head">
                    <h3>最近结果</h3>
                    <div className="note">长内容已压缩显示</div>
                  </div>
                  <div className="panel-body">
                    <div className="receipt-grid status-receipts">
                      <ReceiptCard
                        label="网络检测"
                        receipt={networkReceipt}
                        accent={online === true ? "online" : online === false ? "offline" : "neutral"}
                      />
                      <ReceiptCard
                        label="保存状态"
                        receipt={saveReceipt}
                        accent="save"
                      />
                    </div>
                  </div>
                </div>
              </section>
            ) : (
              <section key="settings" className="page active settings-page">
                <div className="panel">
                  <div className="panel-head">
                    <h3>主题</h3>
                    <div className="note">重启后保留</div>
                  </div>
                  <div className="panel-body">
                    <div className="theme-grid">
                      {themes.map((item) => (
                        <button
                          key={item.id}
                          className={`theme-option ${theme === item.id ? "active" : ""}`}
                          type="button"
                          onClick={() => setTheme(item.id)}
                        >
                          <span className={`theme-swatch ${item.id}`} aria-hidden="true" />
                          <span className="theme-copy">
                            <strong>{item.label}</strong>
                            <span>{item.detail}</span>
                          </span>
                          {theme === item.id ? <CheckCircle2 size={16} /> : <Palette size={16} />}
                        </button>
                      ))}
                    </div>
                  </div>
                </div>

                <div className="panel">
                  <div className="panel-head">
                    <h3>运行偏好</h3>
                    <div className="note">轻量后台</div>
                  </div>
                  <div className="panel-body">
                    <div className="setting-form-grid">
                      <Field label="重试间隔(秒)">
                        <input
                          type="number"
                          min="15"
                          max="3600"
                          value={form.retry_seconds}
                          onChange={(e) => updateField("retry_seconds", Number(e.target.value || 15))}
                        />
                      </Field>
                      <Field label="在线巡检(秒)">
                        <input
                          type="number"
                          min="60"
                          max="3600"
                          value={form.online_check_seconds}
                          onChange={(e) => updateField("online_check_seconds", Number(e.target.value || 60))}
                        />
                      </Field>
                    </div>
                    <div className="summary">
                      <Row label="自动重连" value={guardLabel} />
                      <Row label="开机启动" value={startupEnabled ? "已开启" : "已关闭"} />
                      <Row label="重试间隔" value={summary.retry} />
                      <Row label="在线巡检" value={summary.onlineCheck} />
                      <Row label="软件版本" value={summary.version} />
                    </div>
                    <div className="setting-switch-row">
                      <div>
                        <strong>开机启动</strong>
                        <span>登录 Windows 后自动启动客户端，方便后台守护网络</span>
                      </div>
                      <label className="switch">
                        <input
                          type="checkbox"
                          checked={startupEnabled}
                          onChange={(e) => invoke("set_startup_enabled_cmd", { enabled: e.target.checked })}
                        />
                        <span />
                      </label>
                    </div>
                  </div>
                </div>

                <div className="panel settings-network-panel">
                  <div className="panel-head">
                    <h3>高级网络</h3>
                    <div className="note">按需诊断，不自动扫描</div>
                  </div>
                  <div className="panel-body">
                    <NetworkOutletPicker
                      bindIp={form.bind_ip}
                      interfaces={networkInterfaces}
                      loading={networkInterfacesLoading}
                      error={networkInterfacesError}
                      onChange={(value) => updateField("bind_ip", value)}
                      onRefresh={() => refreshNetworkInterfaces({ force: true })}
                    />
                    <div className="network-summary-grid">
                      <Row label="当前出口" value={summary.bindIp} />
                      <Row label="Portal" value={summary.portal} />
                      <Row label="ac_id" value={form.ac_id || "-"} />
                      <Row label="客户端 IP" value={form.user_ip || "-"} />
                    </div>

                    <details className="advanced network-config">
                      <summary>Portal 与诊断参数</summary>
                      <div className="advanced-body">
                        <div className="grid two-col">
                          <Field label="Portal 地址">
                            <input value={form.portal_url} onChange={(e) => updateField("portal_url", e.target.value)} />
                          </Field>
                          <Field label="探测地址">
                            <input value={form.probe_url} onChange={(e) => updateField("probe_url", e.target.value)} />
                          </Field>
                          <Field label="ac_id">
                            <input value={form.ac_id} onChange={(e) => updateField("ac_id", e.target.value)} />
                          </Field>
                          <Field label="客户端 IP">
                            <input value={form.user_ip} onChange={(e) => updateField("user_ip", e.target.value)} />
                          </Field>
                        </div>
                        <div className="checks compact">
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
                    </details>

                    <div className="advanced-actions network-actions">
                      <button className="action soft" disabled={taskRunning} onClick={() => invoke("detect_portal_cmd")}>
                        <SearchCheck size={15} />
                        {taskRunning && lastCommandRef.current === "detect_portal_cmd" ? "探测中" : "自动探测 Portal"}
                      </button>
                      <button className="action soft" disabled={taskRunning} onClick={() => invoke("diagnose_cmd")}>
                        <Bug size={15} />
                        {taskRunning && lastCommandRef.current === "diagnose_cmd" ? "诊断中" : "诊断"}
                      </button>
                      <button className="action soft" disabled={taskRunning} onClick={() => invoke("check_status_cmd")}>
                        <SearchCheck size={15} />
                        {taskRunning && lastCommandRef.current === "check_status_cmd" ? "检测中" : "检测"}
                      </button>
                      <button className="action soft" disabled={taskRunning} onClick={() => invoke("reconnect_self_test_cmd")}>
                        <RefreshCw size={15} />
                        {taskRunning && lastCommandRef.current === "reconnect_self_test_cmd" ? "自测中" : "重连自测"}
                      </button>
                      <button className="action" disabled={taskRunning} onClick={() => invoke("save_config_cmd")}>
                        <Save size={15} />
                        {taskRunning && lastCommandRef.current === "save_config_cmd" ? "保存中" : "保存"}
                      </button>
                    </div>
                  </div>
                </div>

                <div className="panel">
                  <div className="panel-head">
                    <h3>项目</h3>
                    <div className="note">源码与更新</div>
                  </div>
                  <div className="panel-body">
                    <div className="project-actions">
                      <button className="repo-link" type="button" onClick={openRepository}>
                        <span className="repo-icon" aria-hidden="true">
                          <Github size={17} />
                        </span>
                        <span className="repo-copy">
                          <strong>GitHub 仓库</strong>
                          <span>查看源码和提交反馈</span>
                        </span>
                      </button>
                      <button className="action soft update-button" type="button" onClick={checkUpdates}>
                        <RefreshCw size={15} />
                        {updating ? "更新中" : "检查更新"}
                      </button>
                    </div>
                  </div>
                </div>
              </section>
            )}
          </div>

          <div className="status">{compactStatus(statusText)}</div>
        </main>
      </div>
      {updateNotice.visible ? (
        <UpdateNotice
          currentVersion={appVersion}
          update={updateNotice.update}
          updating={updating}
          onInstall={() => installUpdate(updateNotice.update)}
          onLater={() => {
            setUpdateNotice(EMPTY_UPDATE_NOTICE);
            setStatusText(`发现新版本 v${updateNotice.update?.version || ""}，已暂不安装`);
          }}
        />
      ) : null}
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

function NetworkMonitorPanel({ monitor, compact = false, online = null, onlineLabel = "待检测", currentIp = "" }) {
  const totalSpeed = monitor.totalDownBps + monitor.totalUpBps;
  const { linePath, fillPath } = sparklinePaths(monitor.samples);
  const topAdapters = monitor.adapters.slice(0, 6);
  const stateClass = online === true ? "online" : online === false ? "offline" : "idle";

  return (
    <div className={`network-monitor panel ${compact ? "compact" : ""}`}>
      <div className="network-monitor-hero">
        <div>
          <div className="eyebrow">实时流量</div>
          <div className="traffic-speed">{formatSpeed(totalSpeed)}</div>
          <div className="traffic-sub">
            下载 {formatSpeed(monitor.totalDownBps)} · 上传 {formatSpeed(monitor.totalUpBps)}
          </div>
        </div>
        <div className={`traffic-state-banner ${stateClass}`}>
          <span aria-hidden="true" />
          <strong>
            {online === true
              ? `已连接${currentIp ? ` IP: ${currentIp}` : ""}`
              : online === false
                ? "未连接网络"
                : onlineLabel}
          </strong>
        </div>
      </div>

      <div className="traffic-wave" aria-label="最近网速变化">
        <svg viewBox="0 0 520 110" role="img" preserveAspectRatio="none">
          <defs>
            <linearGradient id="traffic-fill" x1="0" x2="0" y1="0" y2="1">
              <stop offset="0%" stopColor="rgba(59, 130, 246, 0.07)" />
              <stop offset="100%" stopColor="rgba(59, 130, 246, 0)" />
            </linearGradient>
          </defs>
          <polyline className="traffic-grid-line" points="0,28 520,28" />
          <polyline className="traffic-grid-line" points="0,72 520,72" />
          <path className="traffic-fill" d={fillPath} />
          <path className="traffic-line" d={linePath} />
        </svg>
      </div>

      <div className="traffic-stats">
        <div>
          <span>峰值</span>
          <strong>{formatSpeed(monitor.peakBps)}</strong>
        </div>
        <div title={NETWORK_TOTAL_HINT}>
          <span>本机网卡累计</span>
          <strong>{formatBytes(monitor.totalBytes)}</strong>
          <small>非套餐流量</small>
        </div>
        <div>
          <span>活跃网卡</span>
          <strong>{monitor.activeAdapters}</strong>
        </div>
      </div>

      <div className="adapter-list">
        {topAdapters.length ? (
          topAdapters.map((adapter) => (
            <div className="adapter-row" key={adapter.name}>
              <div className="adapter-main">
                <strong>{adapter.name}</strong>
                <span>
                  {adapter.kind} · {adapter.recommendation}
                </span>
              </div>
              <div className="adapter-badges">
                {adapter.is_likely_srun_exit ? <span className="adapter-badge good">校园网候选</span> : null}
                {adapter.is_virtual ? <span className="adapter-badge warn">虚拟网卡</span> : null}
              </div>
              <div className="adapter-speed">
                <span>↓ {formatSpeed(adapter.downBps)}</span>
                <span>↑ {formatSpeed(adapter.upBps)}</span>
              </div>
            </div>
          ))
        ) : (
          <div className="adapter-empty">暂无网卡流量数据</div>
        )}
      </div>

      {monitor.error ? <div className="network-monitor-error">{monitor.error}</div> : null}
    </div>
  );
}

function UpdateNotice({ currentVersion, update, updating, onInstall, onLater }) {
  const latestVersion = update?.version || "-";
  const body = extractUpdateSummary(update);

  return (
    <div className="update-backdrop" role="presentation">
      <div className="update-notice" role="dialog" aria-modal="true" aria-label="发现新版本">
        <div className="update-notice-head">
          <div>
            <div className="eyebrow">版本更新</div>
            <h3>发现新版本</h3>
          </div>
          <span className="pill watch">v{latestVersion}</span>
        </div>

        <div className="update-version-grid">
          <div>
            <span>当前版本</span>
            <strong>v{currentVersion || PACKAGE_VERSION}</strong>
          </div>
          <div>
            <span>最新版本</span>
            <strong>v{latestVersion}</strong>
          </div>
        </div>

        <div className="update-summary">
          <span>更新说明</span>
          <p>{body || "此版本包含稳定性和体验更新"}</p>
        </div>

        <div className="update-actions">
          <button className="action primary" type="button" onClick={onInstall} disabled={updating}>
            <RefreshCw size={15} />
            {updating ? "更新中" : "立即更新"}
          </button>
          <button className="action soft" type="button" onClick={onLater} disabled={updating}>
            稍后
          </button>
        </div>
      </div>
    </div>
  );
}

function extractUpdateSummary(update) {
  const text =
    update?.body ||
    update?.notes ||
    update?.releaseNotes ||
    update?.rawJson?.body ||
    update?.rawJson?.notes ||
    "";
  const clean = String(text)
    .replace(/<[^>]*>/g, " ")
    .replace(/[#*_`>-]/g, " ")
    .replace(/\s+/g, " ")
    .trim();
  return clean.length <= 120 ? clean : `${clean.slice(0, 120)}...`;
}

function NetworkOutletPicker({ bindIp, interfaces, loading, error, onChange, onRefresh }) {
  const [showAllInterfaces, setShowAllInterfaces] = useState(false);
  const selected = bindIp || "";
  const selectedInterface = interfaces.find((item) => item.ip === bindIp);
  const hasSavedBindIp = Boolean(bindIp) && !selectedInterface;
  const visibleInterfaces = showAllInterfaces
    ? interfaces
    : interfaces.filter((item) => !isNoisyLoginInterface(item));
  const selectedVisible =
    !selectedInterface || visibleInterfaces.some((item) => item.ip === selectedInterface.ip);

  return (
    <div className="network-outlet">
      <div className="network-outlet-head">
        <div>
          <div className="field-label">登录出口</div>
          <p>通常保持自动即可；只有登录异常时再手动选择</p>
        </div>
        <button className="action soft compact" type="button" onClick={onRefresh} disabled={loading}>
          <RefreshCw size={14} />
          {loading ? "刷新中" : "刷新"}
        </button>
      </div>

      <select value={selected} onChange={(event) => onChange(event.target.value)}>
        <option value="">自动选择（推荐）</option>
        {hasSavedBindIp ? <option value={bindIp}>已保存出口 / {bindIp}</option> : null}
        {selectedInterface && !selectedVisible ? (
          <option value={selectedInterface.ip}>{formatInterfaceOption(selectedInterface)}</option>
        ) : null}
        {visibleInterfaces.map((item) => (
          <option key={`${item.interface_alias}-${item.ip}`} value={item.ip}>
            {formatInterfaceOption(item)}
          </option>
        ))}
      </select>

      {interfaces.length ? (
        <label className="network-outlet-toggle">
          <input
            type="checkbox"
            checked={showAllInterfaces}
            onChange={(event) => setShowAllInterfaces(event.target.checked)}
          />
          <span>
            显示全部网卡
            {!showAllInterfaces && interfaces.length > visibleInterfaces.length
              ? `（已隐藏 ${interfaces.length - visibleInterfaces.length} 个无效/临时网卡）`
              : ""}
          </span>
        </label>
      ) : null}

      <div className="network-outlet-status">
        {selectedInterface ? (
          <>
            <span className={`adapter-badge ${selectedInterface.is_likely_campus ? "good" : selectedInterface.is_virtual ? "warn" : ""}`}>
              {selectedInterface.recommendation}
            </span>
            <span>{selectedInterface.interface_alias}</span>
            <span>{selectedInterface.ip}</span>
          </>
        ) : hasSavedBindIp ? (
          <>
            <span className="adapter-badge">已保存</span>
            <span>{bindIp}</span>
          </>
        ) : (
          <span>当前按系统路由和校园网地址自动选择</span>
        )}
      </div>

      {error ? <div className="network-outlet-error">{error}</div> : null}
    </div>
  );
}

function isNoisyLoginInterface(item) {
  const ip = String(item?.ip || "");
  const name = `${item?.interface_alias || ""} ${item?.interface_description || ""}`.toLowerCase();
  return (
    ip.startsWith("127.") ||
    ip.startsWith("169.254.") ||
    ip.startsWith("0.") ||
    name.includes("loopback") ||
    name.includes("pseudo-interface") ||
    name.includes("bluetooth")
  );
}

function formatInterfaceOption(item) {
  const tags = [];
  if (item.is_likely_campus) tags.push("可能是校园网");
  if (item.is_virtual) tags.push("虚拟网卡");
  if (item.route_to_portal) tags.push("Portal 路由");
  if (item.route_to_internet) tags.push("默认出口");
  const suffix = tags.length ? `（${tags.join("，")}）` : "";
  return `${item.interface_alias || "网络接口"} / ${item.ip}${suffix}`;
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

function compactStatus(status) {
  if (!status) return "Ready";
  const text = String(status);
  if (text.startsWith("诊断\n")) {
    const conclusion = text.match(/结论：([^\n]+)/)?.[1];
    const rad = text.match(/rad_user_info：([^\n]+)/)?.[1];
    const challengeOk = /Challenge：challenge ok/i.test(text);
    const parts = [];
    if (conclusion) parts.push(conclusion.replace(/，/g, " / "));
    if (rad) parts.push(rad);
    if (challengeOk) parts.push("Challenge 正常");
    const result = parts.length ? parts.join(" / ") : "诊断完成";
    return result.length <= 54 ? result : `${result.slice(0, 54)}...`;
  }
  if (text.length <= 96) return text;
  return `${text.slice(0, 96)}...`;
}

function compactEventText(text) {
  const compact = compactStatus(text);
  return compact.length <= 80 ? compact : `${compact.slice(0, 80)}...`;
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
      <ReceiptDetail detail={receipt.detail} />
      <div className="receipt-meta">
        <span>时间</span>
        <strong>{formatTime(receipt.at)}</strong>
      </div>
    </div>
  );
}

function ReceiptListItem({ label, receipt, accent }) {
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
    <div className={`receipt-list-item ${accent} ${receipt.state}`}>
      <span className={`receipt-list-icon ${receipt.state}`} aria-hidden="true">
        <Icon size={14} />
      </span>
      <div className="receipt-list-copy">
        <span>{label}</span>
        <strong>{receipt.title}</strong>
      </div>
      <time>{formatTime(receipt.at)}</time>
    </div>
  );
}

function ReceiptDetail({ detail }) {
  const diagnostic = parseDiagnostic(detail);
  if (!diagnostic) {
    return <div className="receipt-detail">{detail}</div>;
  }

  const failedProbes = diagnostic.probes.filter((line) => line.includes("失败")).length;
  const challengeOk = /^challenge ok/i.test(diagnostic.challenge);

  return (
    <div className="diagnostic-detail">
      <div className="diagnostic-conclusion">
        <span className={`diagnostic-dot ${diagnostic.online ? "online" : "warning"}`} />
        <div>
          <strong>{diagnostic.conclusion || "诊断完成"}</strong>
          <span>{diagnostic.radUserInfo || "状态未知"}</span>
        </div>
      </div>

      <div className="diagnostic-grid">
        <DiagnosticItem label="Portal" value={diagnostic.portal} />
        <DiagnosticItem label="ac_id" value={diagnostic.acId} />
        <DiagnosticItem label="登录出口" value={diagnostic.loginOutlet} />
        <DiagnosticItem label="登录 IP" value={diagnostic.loginIp} />
        <DiagnosticItem label="当前 IP" value={diagnostic.currentIp} />
        <DiagnosticItem label="网卡摘要" value={compactDiagnosticValue(diagnostic.interfaceSummary)} />
        <DiagnosticItem label="守护状态" value={diagnostic.guard} />
        <DiagnosticItem label="探测失败" value={`${failedProbes}/${diagnostic.probes.length || 0}`} />
      </div>

      {diagnostic.note ? <div className="diagnostic-note">{diagnostic.note}</div> : null}
      {diagnostic.networkPath ? (
        <div className="diagnostic-note compact" title={diagnostic.networkPath}>
          网络路径：{compactNetworkPath(diagnostic.networkPath)}
        </div>
      ) : null}

      <div className={`diagnostic-challenge ${challengeOk ? "ok" : "bad"}`}>
        <span>{challengeOk ? "Challenge 正常" : "Challenge 失败"}</span>
        <code>{diagnostic.challenge || "-"}</code>
      </div>

      {diagnostic.probes.length ? (
        <details className="diagnostic-probes">
          <summary>查看探测明细</summary>
          <div>
            {diagnostic.probes.map((line, index) => (
              <code key={`${line}-${index}`}>{line.replace(/^- /, "")}</code>
            ))}
          </div>
        </details>
      ) : null}
    </div>
  );
}

function DiagnosticItem({ label, value }) {
  return (
    <div className="diagnostic-item">
      <span>{label}</span>
      <strong>{value || "-"}</strong>
    </div>
  );
}

function compactNetworkPath(value) {
  const text = String(value || "");
  if (/Portal 路由可能经过/.test(text)) return "Portal 可能经过虚拟网卡/TUN，建议校园网网段直连";
  if (/检测到可能的 TUN|虚拟网卡/.test(text)) return "检测到虚拟网卡/TUN；仅系统代理不影响直连";
  if (/系统代理已开启/.test(text)) return "系统代理已开启；SRUN 请求仍按程序直连";
  if (/系统代理未开启/.test(text)) return "系统代理未开启";
  return text.length <= 42 ? text : `${text.slice(0, 42)}...`;
}

function compactDiagnosticValue(value) {
  const text = String(value || "");
  return text.length <= 42 ? text : `${text.slice(0, 42)}...`;
}

function parseDiagnostic(detail) {
  if (!detail || !String(detail).startsWith("诊断\n")) return null;
  const text = String(detail);
  const lines = text.split(/\r?\n/);
  const probesStart = lines.findIndex((line) => line.trim() === "探测明细：");
  const probeLines = probesStart >= 0 ? lines.slice(probesStart + 1).filter(Boolean) : [];

  const pick = (label) => text.match(new RegExp(`${label}：([^\\n]+)`))?.[1]?.trim() || "";
  const challengeMatch = text.match(/Challenge：([\s\S]*?)(?:\n探测明细：|$)/);

  return {
    conclusion: pick("结论"),
    portal: pick("Portal"),
    acId: pick("ac_id"),
    loginOutlet: pick("登录出口选择"),
    loginIp: pick("登录使用 IP"),
    currentIp: pick("当前校园网 IP") || pick("系统默认出口 IP"),
    networkPath: pick("网络路径") || pick("VPN/代理"),
    interfaceSummary: pick("网卡摘要"),
    note: text.match(/提示：([^\n]+)/)?.[1]?.trim() || "",
    radUserInfo: pick("rad_user_info"),
    guard: pick("自动重连守护"),
    challenge: challengeMatch?.[1]?.trim() || "",
    probes: probeLines,
    online: /rad_user_info：online/.test(text) || /已在线/.test(text),
  };
}

createRoot(document.getElementById("root")).render(<App />);
