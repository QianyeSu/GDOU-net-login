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
  Download,
  Eye,
  EyeOff,
  Github,
  Globe,
  Info,
  LogIn,
  Moon,
  Network,
  Power,
  RefreshCw,
  Save,
  SearchCheck,
  Settings,
  Settings2,
  Sliders,
  Sun,
  Upload,
  Wifi,
  X,
  XCircle,
  Zap,
} from "lucide-react";
import "./styles.css";

const REPOSITORY_URL = "https://github.com/QianyeSu/GDOU-net-login";
const PACKAGE_VERSION = typeof __APP_VERSION__ !== "undefined" ? __APP_VERSION__ : "0.1.7";
const THEME_STORAGE_KEY = "gdou-theme-mode";

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

function getInvoke() {
  if (typeof window === "undefined") return null;
  if (window.__TAURI_INTERNALS__) return tauriInvoke;
  return window.__TAURI__?.core?.invoke || window.__TAURI__?.invoke || window.tauri?.invoke;
}

function getListen() {
  if (typeof window === "undefined") return null;
  if (window.__TAURI_INTERNALS__) return tauriListen;
  return window.__TAURI__?.event?.listen;
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

function formatSecondsToTimer(totalSeconds) {
  const hours = Math.floor(totalSeconds / 3600);
  const minutes = Math.floor((totalSeconds % 3600) / 60);
  const seconds = totalSeconds % 60;
  if (hours > 0) {
    return `${hours}:${minutes.toString().padStart(2, "0")}:${seconds.toString().padStart(2, "0")}`;
  }
  return `${minutes}:${seconds.toString().padStart(2, "0")}`;
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

function App() {
  const [theme, setTheme] = useState(() => {
    return localStorage.getItem(THEME_STORAGE_KEY) || "light";
  });
  const [form, setForm] = useState(defaultForm);
  const [online, setOnline] = useState(null);
  const [statusText, setStatusText] = useState("就绪");
  const [taskRunning, setTaskRunning] = useState(false);
  const [startupEnabled, setStartupEnabled] = useState(false);
  const [appVersion, setAppVersion] = useState(PACKAGE_VERSION);
  const [toasts, setToasts] = useState([]);

  // Settings Modal State
  const [showSettingsModal, setShowSettingsModal] = useState(false);
  const [settingsTab, setSettingsTab] = useState("general");
  const [lastDiagDetail, setLastDiagDetail] = useState("");

  // Traffic & Duration Monitoring State
  const [onlineDuration, setOnlineDuration] = useState(0);
  const [totalTrafficBytes, setTotalTrafficBytes] = useState(0);
  const [uploadSpeedBps, setUploadSpeedBps] = useState(0);
  const [downloadSpeedBps, setDownloadSpeedBps] = useState(0);

  // Mouse Hover Point on Traffic Waveform
  const [hoverPoint, setHoverPoint] = useState(null);
  const hoverPointRef = useRef(null);
  hoverPointRef.current = hoverPoint;

  // Network Interfaces State
  const [networkInterfaces, setNetworkInterfaces] = useState([]);
  const [networkInterfacesLoading, setNetworkInterfacesLoading] = useState(false);
  const [networkInterfacesError, setNetworkInterfacesError] = useState("");

  // Software Update State
  const [updating, setUpdating] = useState(false);
  const [updateNotice, setUpdateNotice] = useState({ visible: false, update: null, source: "manual" });

  const lastCommandRef = useRef("load_state_cmd");
  const autoSaveReadyRef = useRef(false);
  const autoSaveSnapshotRef = useRef(autoSaveSnapshot(defaultForm));
  const previousNetworkSnapshotRef = useRef(null);
  const canvasRef = useRef(null);

  // History buffer queue for waveform (50 data points)
  const DATA_POINTS_COUNT = 50;
  const trafficHistoryRef = useRef({
    upload: new Array(DATA_POINTS_COUNT).fill(0),
    download: new Array(DATA_POINTS_COUNT).fill(0),
    timestamps: new Array(DATA_POINTS_COUNT).fill(""),
  });

  // Apply Theme Preference
  useEffect(() => {
    localStorage.setItem(THEME_STORAGE_KEY, theme);
    if (theme === "dark") {
      document.body.classList.add("dark");
    } else {
      document.body.classList.remove("dark");
    }
  }, [theme]);

  // Toast Notification Helper
  const showToast = (message, type = "success") => {
    const id = Date.now() + Math.random();
    setToasts((prev) => [...prev, { id, message, type }]);
    setTimeout(() => {
      setToasts((prev) => prev.filter((t) => t.id !== id));
    }, 3200);
  };

  const updateField = (name, value) => {
    setForm((prev) => ({ ...prev, [name]: value }));
  };

  // Online Session Duration Timer
  useEffect(() => {
    let timer = null;
    if (online === true) {
      timer = setInterval(() => {
        setOnlineDuration((prev) => prev + 1);
      }, 1000);
    } else {
      setOnlineDuration(0);
    }
    return () => {
      if (timer) clearInterval(timer);
    };
  }, [online]);

  // Initial Load & Event Listeners
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

        getTauriVersion()
          .then((v) => {
            if (mounted && v) setAppVersion(v);
          })
          .catch(() => {});

        const invoke = getInvoke();
        if (invoke) {
          const initialState = await invoke("load_state_cmd");
          if (initialState?.config) {
            autoSaveSnapshotRef.current = autoSaveSnapshot({
              ...defaultForm,
              ...initialState.config,
              accept_terms: true,
            });
          }
          applyResponse(initialState);
          autoSaveReadyRef.current = true;
        } else {
          setStatusText("预览模式（未连接后端）");
          autoSaveReadyRef.current = true;
        }
      } catch (err) {
        const msg = String(err?.message || err);
        setStatusText(msg);
        showToast(msg, "error");
      }
    })();

    return () => {
      mounted = false;
    };
  }, []);

  // Autosave Configuration on Change
  useEffect(() => {
    const snapshot = autoSaveSnapshot(form);
    if (!autoSaveReadyRef.current) {
      autoSaveSnapshotRef.current = snapshot;
      return;
    }
    if (snapshot === autoSaveSnapshotRef.current) return;

    const invoke = getInvoke();
    if (!invoke) return;

    const timer = window.setTimeout(async () => {
      const requestForm = { ...form, accept_terms: true };
      try {
        await invoke("autosave_config_cmd", {
          config: requestForm,
          ...requestForm,
        });
        autoSaveSnapshotRef.current = snapshot;
      } catch (err) {
        console.error("Autosave error:", err);
      }
    }, 1200);

    return () => window.clearTimeout(timer);
  }, [form]);

  // Draw Realtime Traffic Waveform Canvas
  const drawTrafficChart = (hoverIdx = null) => {
    const canvas = canvasRef.current;
    if (!canvas) return;
    const ctx = canvas.getContext("2d");
    if (!ctx) return;

    const width = canvas.width;
    const height = canvas.height;
    const uploadData = trafficHistoryRef.current.upload;
    const downloadData = trafficHistoryRef.current.download;

    ctx.clearRect(0, 0, width, height);

    // Draw dashed grid lines
    ctx.strokeStyle = theme === "dark" ? "rgba(255, 255, 255, 0.08)" : "rgba(0, 0, 0, 0.06)";
    ctx.lineWidth = 1;
    ctx.setLineDash([3, 5]);
    for (let i = 1; i <= 3; i++) {
      const y = (height / 4) * i;
      ctx.beginPath();
      ctx.moveTo(0, y);
      ctx.lineTo(width, y);
      ctx.stroke();
    }
    ctx.setLineDash([]);

    const stepX = width / (DATA_POINTS_COUNT - 1);
    const maxValue = Math.max(...uploadData, ...downloadData, 50 * 1024); // 50KB minimum baseline

    // 1. Draw download curve (Green)
    ctx.strokeStyle = "#4ade80";
    ctx.lineWidth = 2.2;
    ctx.shadowColor = "#4ade80";
    ctx.shadowBlur = 6;
    ctx.beginPath();
    downloadData.forEach((value, index) => {
      const x = index * stepX;
      const y = height - (value / maxValue) * (height - 10) - 5;
      if (index === 0) ctx.moveTo(x, y);
      else ctx.lineTo(x, y);
    });
    ctx.stroke();
    ctx.shadowBlur = 0;

    // Download area gradient fill
    ctx.lineTo(width, height);
    ctx.lineTo(0, height);
    ctx.closePath();
    const downloadGradient = ctx.createLinearGradient(0, 0, 0, height);
    downloadGradient.addColorStop(0, "rgba(74, 222, 128, 0.22)");
    downloadGradient.addColorStop(1, "rgba(74, 222, 128, 0)");
    ctx.fillStyle = downloadGradient;
    ctx.fill();

    // 2. Draw upload curve (Red)
    ctx.strokeStyle = "#f87171";
    ctx.lineWidth = 2.0;
    ctx.shadowColor = "#f87171";
    ctx.shadowBlur = 6;
    ctx.beginPath();
    uploadData.forEach((value, index) => {
      const x = index * stepX;
      const y = height - (value / maxValue) * (height - 10) - 5;
      if (index === 0) ctx.moveTo(x, y);
      else ctx.lineTo(x, y);
    });
    ctx.stroke();
    ctx.shadowBlur = 0;

    // Upload area gradient fill
    ctx.lineTo(width, height);
    ctx.lineTo(0, height);
    ctx.closePath();
    const uploadGradient = ctx.createLinearGradient(0, 0, 0, height);
    uploadGradient.addColorStop(0, "rgba(248, 113, 113, 0.16)");
    uploadGradient.addColorStop(1, "rgba(248, 113, 113, 0)");
    ctx.fillStyle = uploadGradient;
    ctx.fill();

    // 3. Draw hover guide line & highlighted focal points
    const effectiveHoverIdx = hoverIdx !== null ? hoverIdx : hoverPointRef.current?.index;
    if (effectiveHoverIdx !== undefined && effectiveHoverIdx !== null && effectiveHoverIdx >= 0 && effectiveHoverIdx < DATA_POINTS_COUNT) {
      const hx = effectiveHoverIdx * stepX;
      const downY = height - (downloadData[effectiveHoverIdx] / maxValue) * (height - 10) - 5;
      const upY = height - (uploadData[effectiveHoverIdx] / maxValue) * (height - 10) - 5;

      // Vertical guide line
      ctx.save();
      ctx.beginPath();
      ctx.moveTo(hx, 0);
      ctx.lineTo(hx, height);
      ctx.strokeStyle = theme === "dark" ? "rgba(255, 255, 255, 0.35)" : "rgba(0, 0, 0, 0.22)";
      ctx.lineWidth = 1;
      ctx.setLineDash([3, 3]);
      ctx.stroke();
      ctx.restore();

      // Download focal point
      ctx.beginPath();
      ctx.arc(hx, downY, 4, 0, Math.PI * 2);
      ctx.fillStyle = "#4ade80";
      ctx.fill();
      ctx.lineWidth = 2;
      ctx.strokeStyle = "#ffffff";
      ctx.stroke();

      // Upload focal point
      ctx.beginPath();
      ctx.arc(hx, upY, 4, 0, Math.PI * 2);
      ctx.fillStyle = "#f87171";
      ctx.fill();
      ctx.lineWidth = 2;
      ctx.strokeStyle = "#ffffff";
      ctx.stroke();
    }
  };

  // Mouse movement on Canvas
  const handleCanvasMouseMove = (e) => {
    const canvas = canvasRef.current;
    if (!canvas) return;
    const rect = canvas.getBoundingClientRect();
    const mouseX = Math.max(0, Math.min(rect.width, e.clientX - rect.left));
    const mouseY = Math.max(0, Math.min(rect.height, e.clientY - rect.top));
    const ratio = mouseX / rect.width;
    const index = Math.max(0, Math.min(DATA_POINTS_COUNT - 1, Math.round(ratio * (DATA_POINTS_COUNT - 1))));

    const downVal = trafficHistoryRef.current.download[index] || 0;
    const upVal = trafficHistoryRef.current.upload[index] || 0;
    const timeVal = trafficHistoryRef.current.timestamps[index] || new Date().toLocaleTimeString("zh-CN", { hour12: false });

    setHoverPoint({
      active: true,
      x: mouseX,
      y: mouseY,
      index,
      downBps: downVal,
      upBps: upVal,
      timeStr: timeVal,
    });

    drawTrafficChart(index);
  };

  const handleCanvasMouseLeave = () => {
    setHoverPoint(null);
    drawTrafficChart(null);
  };

  // Realtime Traffic Polling Loop (Optimized with Zero-CPU Background Throttling)
  useEffect(() => {
    let timer = null;
    let stopped = false;

    async function pollTraffic() {
      if (stopped) return;

      // Pause high-frequency polling and canvas rendering when window is minimized/hidden in tray
      if (typeof document !== "undefined" && document.hidden) {
        return;
      }

      const nowStr = new Date().toLocaleTimeString("zh-CN", { hour12: false });
      const invoke = getInvoke();
      if (!invoke) {
        // Preview simulation fallback
        if (online === true) {
          const down = Math.random() * 800 * 1024 + 100 * 1024;
          const up = Math.random() * 200 * 1024 + 40 * 1024;
          setDownloadSpeedBps(down);
          setUploadSpeedBps(up);
          setTotalTrafficBytes((prev) => prev + down + up);
          trafficHistoryRef.current.download.shift();
          trafficHistoryRef.current.download.push(down);
          trafficHistoryRef.current.upload.shift();
          trafficHistoryRef.current.upload.push(up);
          trafficHistoryRef.current.timestamps.shift();
          trafficHistoryRef.current.timestamps.push(nowStr);
        } else {
          setDownloadSpeedBps(0);
          setUploadSpeedBps(0);
          trafficHistoryRef.current.download.shift();
          trafficHistoryRef.current.download.push(0);
          trafficHistoryRef.current.upload.shift();
          trafficHistoryRef.current.upload.push(0);
          trafficHistoryRef.current.timestamps.shift();
          trafficHistoryRef.current.timestamps.push(nowStr);
        }
        drawTrafficChart();
        return;
      }

      try {
        const snapshot = await invoke("network_monitor_snapshot_cmd");
        const adapters = Array.isArray(snapshot?.adapters) ? snapshot.adapters : [];
        const previousSnapshot = previousNetworkSnapshotRef.current;
        const elapsedSeconds = Math.max(
          0.5,
          (Number(snapshot?.timestamp_ms || 0) - Number(previousSnapshot?.timestamp_ms || 0) || 1000) / 1000
        );

        let totalDown = 0;
        let totalUp = 0;
        let totalBytes = 0;

        adapters.forEach((adapter) => {
          const backendDownDelta = Number(adapter.received_per_refresh || 0);
          const backendUpDelta = Number(adapter.transmitted_per_refresh || 0);
          totalDown += backendDownDelta / elapsedSeconds;
          totalUp += backendUpDelta / elapsedSeconds;
          totalBytes += Number(adapter.total_bytes || 0);
        });

        previousNetworkSnapshotRef.current = snapshot;

        setDownloadSpeedBps(totalDown);
        setUploadSpeedBps(totalUp);
        setTotalTrafficBytes(totalBytes);

        trafficHistoryRef.current.download.shift();
        trafficHistoryRef.current.download.push(totalDown);
        trafficHistoryRef.current.upload.shift();
        trafficHistoryRef.current.upload.push(totalUp);
        trafficHistoryRef.current.timestamps.shift();
        trafficHistoryRef.current.timestamps.push(nowStr);

        drawTrafficChart();
      } catch (err) {
        console.error("Traffic poll error:", err);
      }
    }

    const handleVisibilityChange = () => {
      if (!document.hidden && !stopped) {
        pollTraffic();
      }
    };
    document.addEventListener("visibilitychange", handleVisibilityChange);

    timer = setInterval(pollTraffic, 1000);
    pollTraffic();

    return () => {
      stopped = true;
      if (timer) clearInterval(timer);
      document.removeEventListener("visibilitychange", handleVisibilityChange);
    };
  }, [online, theme]);

  // Process Backend State Updates
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
    }
    if (typeof result?.auto_reconnect === "boolean") {
      updateField("auto_reconnect", result.auto_reconnect);
    }
    if (typeof result?.startup_enabled === "boolean") {
      setStartupEnabled(result.startup_enabled);
    }
    if (result?.status) {
      setStatusText(result.status);
      setLastDiagDetail(result.status);
    }
  }

  // Generic Tauri IPC Invoke Wrapper
  async function invokeCmd(cmd, args = {}, successToast = "") {
    if (taskRunning && cmd !== "set_auto_reconnect_cmd" && cmd !== "set_startup_enabled_cmd") return;
    try {
      setTaskRunning(true);
      lastCommandRef.current = cmd;
      const invoke = getInvoke();

      if (!invoke) {
        // Preview simulation fallback
        setTimeout(() => {
          if (cmd === "login_cmd") {
            setOnline(true);
            setStatusText("已连接 (预览模式)");
            showToast("连接成功！(预览)", "success");
          } else if (cmd === "logout_cmd") {
            setOnline(false);
            setStatusText("已断开 (预览模式)");
            showToast("已断开连接", "info");
          } else if (cmd === "set_startup_enabled_cmd") {
            setStartupEnabled(args.enabled);
            showToast(args.enabled ? "已开启开机自启" : "已关闭开机自启", "success");
          } else {
            showToast(successToast || "操作完成 (预览)", "success");
          }
          setTaskRunning(false);
        }, 500);
        return;
      }

      const requestForm = { ...form, accept_terms: true };
      const result = await invoke(cmd, {
        config: requestForm,
        ...requestForm,
        ...args,
      });
      applyResponse(result);

      if (cmd === "login_cmd") {
        if (result?.online) {
          showToast("连接成功！", "success");
        } else {
          showToast(result?.status || "登录未确认在线", "warning");
        }
      } else if (cmd === "logout_cmd") {
        showToast("已断开连接", "info");
      } else if (cmd === "save_config_cmd") {
        showToast("配置已成功保存", "success");
      } else if (cmd === "check_status_cmd") {
        showToast(result?.online ? "网络在线正常" : "当前未在线", result?.online ? "success" : "warning");
      } else if (cmd === "reconnect_self_test_cmd") {
        showToast("重连自测完成：" + (result?.online ? "已恢复在线" : "未在线"), result?.online ? "success" : "warning");
      } else if (cmd === "set_startup_enabled_cmd") {
        showToast(args.enabled ? "已启用开机自启" : "已禁用开机自启", "success");
      } else if (successToast) {
        showToast(successToast, "success");
      }
    } catch (err) {
      const msg = String(err?.message || err);
      setStatusText(msg);
      showToast(msg, "error");
    } finally {
      setTaskRunning(false);
    }
  }

  // Refresh Network Interfaces List
  async function refreshInterfaces() {
    const invoke = getInvoke();
    if (!invoke) return;
    setNetworkInterfacesLoading(true);
    setNetworkInterfacesError("");
    try {
      const items = await invoke("list_network_interfaces_cmd", { force: true });
      setNetworkInterfaces(Array.isArray(items) ? items : []);
    } catch (err) {
      setNetworkInterfacesError(String(err?.message || err));
    } finally {
      setNetworkInterfacesLoading(false);
    }
  }

  // Check Software Updates
  async function checkUpdates() {
    const invoke = getInvoke();
    if (!invoke) {
      window.open(`${REPOSITORY_URL}/releases`, "_blank");
      return;
    }
    setUpdating(true);
    showToast("正在检查更新...", "info");
    try {
      const update = await tauriCheckUpdate({ timeout: 12000 });
      if (!update) {
        showToast(`当前已是最新版本 v${appVersion}`, "success");
      } else {
        setUpdateNotice({ visible: true, update, source: "manual" });
        showToast(`发现新版本 v${update.version}`, "success");
      }
    } catch (err) {
      showToast("检查更新失败，请稍后重试", "warning");
    } finally {
      setUpdating(false);
    }
  }

  // Download & Install Update
  async function installUpdate(update) {
    if (!update) return;
    try {
      setUpdating(true);
      setUpdateNotice((prev) => ({ ...prev, visible: false }));
      showToast(`正在下载并安装 v${update.version}...`, "info");
      await update.downloadAndInstall();
      await tauriRelaunch();
    } catch (err) {
      showToast(`更新失败：${String(err?.message || err)}`, "error");
    } finally {
      setUpdating(false);
    }
  }

  const handleStartDrag = (e) => {
    if (
      e.button === 0 &&
      !e.target.closest("button") &&
      !e.target.closest("input") &&
      !e.target.closest("select") &&
      !e.target.closest("textarea") &&
      !e.target.closest("a") &&
      !e.target.closest(".modal-card") &&
      !e.target.closest(".traffic-canvas-wrap") &&
      !e.target.closest(".input-wrapper")
    ) {
      const invoke = getInvoke();
      if (invoke) invoke("start_drag_cmd").catch(() => {});
    }
  };

  const handleStartResize = (e, direction) => {
    if (e.button === 0) {
      e.stopPropagation();
      e.preventDefault();
      const invoke = getInvoke();
      if (invoke) {
        invoke("start_resize_cmd", { direction }).catch(() => {});
      }
    }
  };

  const reconnectPaused = /EasyConnect 已连接/i.test(statusText);
  const statusType = reconnectPaused
    ? "paused"
    : taskRunning && lastCommandRef.current === "login_cmd"
    ? "connecting"
    : online === true
    ? "online"
    : online === false
    ? "offline"
    : "connecting";

  const statusTitle = reconnectPaused
    ? "自动重连已暂停"
    : taskRunning && lastCommandRef.current === "login_cmd"
    ? "正在连接..."
    : online === true
    ? "已连接"
    : online === false
    ? "未连接"
    : "检测中...";

  const statusSub = form.user_ip
    ? `IP: ${form.user_ip}`
    : online === true
    ? "校园网正常在线"
    : "等待登录认证";

  return (
    <>
      {/* Main Container (Integrated Frameless Card with Full Dragging & 8-Way Edge Resizing) */}
      <div className="container" onMouseDown={handleStartDrag}>
        {/* 8-Directional Window Edge Resize Handles */}
        <div className="resize-handle top" onMouseDown={(e) => handleStartResize(e, "top")} />
        <div className="resize-handle bottom" onMouseDown={(e) => handleStartResize(e, "bottom")} />
        <div className="resize-handle left" onMouseDown={(e) => handleStartResize(e, "left")} />
        <div className="resize-handle right" onMouseDown={(e) => handleStartResize(e, "right")} />
        <div className="resize-handle top-left" onMouseDown={(e) => handleStartResize(e, "top-left")} />
        <div className="resize-handle top-right" onMouseDown={(e) => handleStartResize(e, "top-right")} />
        <div className="resize-handle bottom-left" onMouseDown={(e) => handleStartResize(e, "bottom-left")} />
        <div className="resize-handle bottom-right" onMouseDown={(e) => handleStartResize(e, "bottom-right")} />

        <div className="main-card" onMouseDown={handleStartDrag}>
          {/* macOS Frameless Titlebar & Traffic Lights */}
          <div className="macos-titlebar" data-tauri-drag-region onMouseDown={handleStartDrag}>
            <div className="macos-title-drag" data-tauri-drag-region onMouseDown={handleStartDrag} />
            <div className="traffic-lights">
              <button
                type="button"
                className="traffic-light yellow"
                onClick={() => {
                  const invoke = getInvoke();
                  if (invoke) invoke("minimize_window_cmd");
                }}
                title="最小化"
              >
                <span className="light-icon">−</span>
              </button>
              <button
                type="button"
                className="traffic-light green"
                onClick={() => {
                  const invoke = getInvoke();
                  if (invoke) invoke("toggle_maximize_cmd");
                }}
                title="最大化 / 还原"
              >
                <span className="light-icon">+</span>
              </button>
              <button
                type="button"
                className="traffic-light red"
                onClick={() => {
                  const invoke = getInvoke();
                  if (invoke) invoke("close_window_cmd");
                }}
                title="关闭 / 最小化到托盘"
              >
                <span className="light-icon">×</span>
              </button>
            </div>
          </div>

          {/* School Badge & App Header */}
          <div className="logo-section" data-tauri-drag-region onMouseDown={handleStartDrag}>
            <div className="logo-icon-wrap" title="广东海洋大学">
              <SchoolBadgeLogo />
            </div>
            <h2 className="logo-main-title">广东海洋大学</h2>
            <div className="logo-tag-badge">
              <span className="logo-tag-dot" />
              <span>校园网助手</span>
            </div>
          </div>

          {/* Network Status Capsule */}
          <div className={`status-indicator ${statusType}`}>
            <div className="status-dot" />
            <div className="status-info">
              <span className="status-text">{statusTitle}</span>
              <span className="status-ip">{statusSub}</span>
            </div>
          </div>

          {/* Login Form Section */}
          <div className="form-section">
            <div className="input-group">
              <label className="input-label">校园网账号</label>
              <div className="input-wrapper">
                <input
                  type="text"
                  className="input-field"
                  placeholder="请输入学号 / 工号"
                  value={form.username}
                  onChange={(e) => updateField("username", e.target.value)}
                  autoComplete="username"
                />
              </div>
            </div>

            <div className="input-group">
              <label className="input-label">认证密码</label>
              <div className="input-wrapper has-icon">
                <input
                  type={form.show_password ? "text" : "password"}
                  className="input-field"
                  placeholder="请输入校园网密码"
                  value={form.password}
                  onChange={(e) => updateField("password", e.target.value)}
                  autoComplete="current-password"
                />
                <button
                  type="button"
                  className="input-icon-btn"
                  onClick={() => updateField("show_password", !form.show_password)}
                  title={form.show_password ? "隐藏密码" : "显示密码"}
                >
                  {form.show_password ? <EyeOff size={16} /> : <Eye size={16} />}
                </button>
              </div>
            </div>

            <label className="checkbox-group">
              <input
                type="checkbox"
                checked={form.auto_reconnect}
                onChange={(e) => updateField("auto_reconnect", e.target.checked)}
              />
              <span className="checkbox-label">保持后台自动守护与断线重连</span>
            </label>

            {/* Primary Action Button (Connect / Disconnect) */}
            <button
              className={`btn-primary ${online === true ? "connected" : ""} ${taskRunning ? "connecting" : ""}`}
              disabled={taskRunning}
              onClick={() => {
                if (online === true) {
                  invokeCmd("logout_cmd");
                } else {
                  invokeCmd("login_cmd");
                }
              }}
            >
              {taskRunning ? (
                <>
                  <span className="spinner" />
                  <span>{lastCommandRef.current === "logout_cmd" ? "正在断开..." : "正在连接..."}</span>
                </>
              ) : online === true ? (
                <>
                  <Power size={17} />
                  <span>断开连接</span>
                </>
              ) : (
                <>
                  <LogIn size={17} />
                  <span>立即连接</span>
                </>
              )}
            </button>
          </div>

          {/* Realtime Traffic Waveform Card */}
          <div className="traffic-monitor">
            <div className="traffic-header">
              <span className="traffic-title">流量</span>
              <div className="traffic-legend">
                <span className="legend-item">
                  <span className="legend-dot download" />
                  下载
                </span>
                <span className="legend-item">
                  <span className="legend-dot upload" />
                  上传
                </span>
              </div>
            </div>

            <div
              className="traffic-canvas-wrap"
              onMouseMove={handleCanvasMouseMove}
              onMouseLeave={handleCanvasMouseLeave}
            >
              <canvas ref={canvasRef} id="trafficChart" width={380} height={64} />

              {/* Tooltip Hover Overlay (cc-switch style) */}
              {hoverPoint?.active && (
                <div
                  className="chart-tooltip"
                  style={{
                    left: `${Math.max(50, Math.min(350, hoverPoint.x))}px`,
                    top: `${Math.max(25, hoverPoint.y)}px`,
                  }}
                >
                  <div className="chart-tooltip-time">{hoverPoint.timeStr}</div>
                  <div className="chart-tooltip-row">
                    <span className="chart-tooltip-label">
                      <span className="tooltip-dot download" />
                      下载
                    </span>
                    <span className="chart-tooltip-value download">
                      {formatSpeed(hoverPoint.downBps)}
                    </span>
                  </div>
                  <div className="chart-tooltip-row">
                    <span className="chart-tooltip-label">
                      <span className="tooltip-dot upload" />
                      上传
                    </span>
                    <span className="chart-tooltip-value upload">
                      {formatSpeed(hoverPoint.upBps)}
                    </span>
                  </div>
                </div>
              )}
            </div>

            <div className="traffic-stats">
              <div className="traffic-stat">
                <div className="traffic-icon download">
                  <Download size={16} />
                </div>
                <div className="traffic-stat-info">
                  <div className="traffic-value">{formatSpeed(downloadSpeedBps)}</div>
                  <div className="traffic-label">下载速率</div>
                </div>
              </div>

              <div className="traffic-stat">
                <div className="traffic-icon upload">
                  <Upload size={16} />
                </div>
                <div className="traffic-stat-info">
                  <div className="traffic-value">{formatSpeed(uploadSpeedBps)}</div>
                  <div className="traffic-label">上传速率</div>
                </div>
              </div>
            </div>
          </div>

          {/* Session Statistics Cards */}
          <div className="stats-grid">
            <div className="stat-card">
              <div className="stat-value">{formatSecondsToTimer(onlineDuration)}</div>
              <div className="stat-label">本次在线时长</div>
            </div>
            <div className="stat-card">
              <div className="stat-value">{formatBytes(totalTrafficBytes)}</div>
              <div className="stat-label">网卡累计流量</div>
            </div>
          </div>

          {/* Bottom Toolbar */}
          <div className="footer-controls">
            <div className="theme-toggle">
              <button
                className={`theme-btn ${theme === "light" ? "active" : ""}`}
                onClick={() => setTheme("light")}
                title="浅色模式"
              >
                <Sun size={14} />
              </button>
              <button
                className={`theme-btn ${theme === "dark" ? "active" : ""}`}
                onClick={() => setTheme("dark")}
                title="暗色模式"
              >
                <Moon size={14} />
              </button>
            </div>

            <div className="footer-meta">
              {/* Settings Modal Launcher */}
              <button
                className="footer-btn"
                onClick={() => {
                  setShowSettingsModal(true);
                  refreshInterfaces();
                }}
                title="打开设置与工具中心"
              >
                <Settings size={13} />
                <span>设置</span>
              </button>

              <button
                className="footer-icon-link"
                onClick={() => {
                  const invoke = getInvoke();
                  if (invoke) invoke("open_repository_cmd").catch(() => window.open(REPOSITORY_URL, "_blank"));
                  else window.open(REPOSITORY_URL, "_blank");
                }}
                title="查看 GitHub 仓库"
              >
                <Github size={15} />
              </button>

              <button
                className="footer-icon-link"
                onClick={checkUpdates}
                disabled={updating}
                title="检查版本更新"
              >
                <RefreshCw size={14} className={updating ? "spinner" : ""} />
              </button>

              <span className="version-badge">v{appVersion}</span>
            </div>
          </div>
        </div>
      </div>

      {/* Toast Notifications */}
      <div className="toast-container" aria-live="polite">
        {toasts.map((t) => (
          <div key={t.id} className={`toast ${t.type}`}>
            <span className="toast-icon">
              {t.type === "success" && <CheckCircle2 size={15} color="#4ade80" />}
              {t.type === "error" && <XCircle size={15} color="#f87171" />}
              {t.type === "warning" && <AlertTriangle size={15} color="#fbbf24" />}
              {t.type === "info" && <Info size={15} color="#60a5fa" />}
            </span>
            <span className="toast-message">{t.message}</span>
          </div>
        ))}
      </div>

      {/* Unified Settings & Diagnostics Modal */}
      {showSettingsModal && (
        <UnifiedSettingsModal
          activeTab={settingsTab}
          setActiveTab={setSettingsTab}
          form={form}
          updateField={updateField}
          startupEnabled={startupEnabled}
          diagDetail={lastDiagDetail}
          interfaces={networkInterfaces}
          interfacesLoading={networkInterfacesLoading}
          interfacesError={networkInterfacesError}
          onRefreshInterfaces={refreshInterfaces}
          onInvoke={invokeCmd}
          onClose={() => setShowSettingsModal(false)}
          taskRunning={taskRunning}
        />
      )}

      {/* Software Update Notice Modal */}
      {updateNotice.visible && (
        <UpdateNoticeModal
          currentVersion={appVersion}
          update={updateNotice.update}
          updating={updating}
          onInstall={() => installUpdate(updateNotice.update)}
          onLater={() => setUpdateNotice({ visible: false, update: null, source: "manual" })}
        />
      )}
    </>
  );
}

// School Badge SVG Component
function SchoolBadgeLogo() {
  return (
    <svg viewBox="0 0 263 263" xmlns="http://www.w3.org/2000/svg">
      <defs>
        <style>{`.cls-1{fill:#fff;}.cls-1,.cls-2,.cls-3{fill-rule:evenodd;}.cls-2{fill:#fdd000;}.cls-3{fill:#003b90;}`}</style>
      </defs>
      <path
        className="cls-3"
        d="m131.45,0c72.4,0,131.45,59.05,131.45,131.45s-59.05,131.45-131.45,131.45S0,203.85,0,131.45,59.05,0,131.45,0h0Zm0,3.18c70.65,0,128.27,57.62,128.27,128.27s-57.62,128.27-128.27,128.27S3.18,202.1,3.18,131.45,60.8,3.18,131.45,3.18h0Z"
      />
      <path
        className="cls-3"
        d="m130.99,35.22c52.38,0,95.1,42.72,95.1,95.1,0,10.67-1.78,20.94-5.05,30.53-.38,1.12-1.11,2.28-1.61,3.3-1.09,2.25-2.15,4.22-3.65,6.37-1.32,1.9-2.87,3.95-4.66,6.04-4.51,5.25-10.82,10.59-17.91,14.78-11.74,6.94-26.73,10.06-39.95,11.25-4.05.36-7.79.59-11.48.67-4.04.09-7.89.66-10.01.71,1.79.77,4.5,1.15,8.3,1.82,6.52,1.16,16.2,2.05,26.75.73,11.93-1.49,24.98-5.75,36.28-14.32-17.46,20.32-43.33,33.22-72.11,33.22-31.64,0-59.74-15.59-77.05-39.47,1.45-1.09,12.28-8.53,31.77-2.2,3.04.83,8.07,2.84,13.9,5.02,4.18,1.56,8.67,3.28,13.28,4.2,6.32,1.27,12.77,1.2,18.05-1.5-2.86-.11-8.59-.38-16.39-2.6-4.09-1.16-8.66-3.05-13.86-5.31-2.48-1.08-5.18-2.19-7.88-3.59-18.07-9.1-33.71-4.1-43.09-.35-8.78-14.39-13.84-31.27-13.84-49.3,0-52.38,42.72-95.1,95.1-95.1h0Z"
      />
      <path
        className="cls-2"
        d="m13.47,131.94s54.74,24.56,67.86,28.31c1.32.5,8.73,3.24,18.87,6.03,8.07,2.22,18.37,4.34,29.52,4.46,16.55.18,35.21-3.92,53.21-17.61,11.02-8.38,19.05-17.45,25.83-26.32,5.87-7.66,10.89-15.16,15.12-22.09,11-18.04,17.63-31.84,17.63-31.84-8.06.56-59.97,3.42-61.79,3.37,0,0-4.45,18.98-15.03,38.11-6.46,11.68-14.98,23.29-27.89,30.73-5.86,3.38-11.21,5.32-20.22,6.9-3.48.61-7.15,1.18-11.01,1.15,8.9-1.72,28.28-7.49,43.87-25.54,10.22-11.83,18.43-30.24,23.42-50.95-26.2,1.15-47.48,2.13-47.48,2.13,0,0-.83,2.13-2.27,5.66-2.66,6.51-7.83,17.63-16.78,27.89-8.56,9.82-20.6,18.87-36.14,22.63-12.54,3.4-24.9,3.36-36.08,1.8-7.65-1.07-14.67-3.09-20.63-4.8h0Z"
      />
    </svg>
  );
}

// Unified Settings Modal Component
function UnifiedSettingsModal({
  activeTab,
  setActiveTab,
  form,
  updateField,
  startupEnabled,
  diagDetail,
  interfaces,
  interfacesLoading,
  interfacesError,
  onRefreshInterfaces,
  onInvoke,
  onClose,
  taskRunning,
}) {
  const [showAllIfaces, setShowAllIfaces] = useState(false);
  const selectedIp = form.bind_ip || "";
  const selectedIface = interfaces.find((item) => item.ip === form.bind_ip);

  const visibleInterfaces = showAllIfaces
    ? interfaces
    : interfaces.filter((item) => {
        const ip = String(item?.ip || "");
        const name = `${item?.interface_alias || ""} ${item?.interface_description || ""}`.toLowerCase();
        return !ip.startsWith("127.") && !ip.startsWith("169.254.") && !name.includes("bluetooth");
      });

  const diagnostic = useMemo(() => parseDiagnostic(diagDetail), [diagDetail]);
  const challengeOk = /^challenge ok/i.test(diagnostic?.challenge || "");

  return (
    <div className="modal-backdrop" onClick={onClose}>
      <div className="modal-card" onClick={(e) => e.stopPropagation()}>
        {/* Modal Header */}
        <div className="modal-header">
          <div className="modal-title-group">
            <span className="modal-title-icon">
              <Settings2 size={16} />
            </span>
            <span className="modal-title">设置与工具中心</span>
          </div>
          <button className="modal-close-btn" onClick={onClose}>
            <X size={16} />
          </button>
        </div>

        {/* Tab Navigation */}
        <div className="modal-tabs">
          <button
            className={`modal-tab-btn ${activeTab === "general" ? "active" : ""}`}
            onClick={() => setActiveTab("general")}
          >
            <Sliders size={14} />
            <span>常规偏好</span>
          </button>
          <button
            className={`modal-tab-btn ${activeTab === "diagnostic" ? "active" : ""}`}
            onClick={() => {
              setActiveTab("diagnostic");
              if (!diagDetail) onInvoke("diagnose_cmd");
            }}
          >
            <Bug size={14} />
            <span>网络诊断</span>
          </button>
          <button
            className={`modal-tab-btn ${activeTab === "advanced" ? "active" : ""}`}
            onClick={() => {
              setActiveTab("advanced");
              onRefreshInterfaces();
            }}
          >
            <Network size={14} />
            <span>高级网卡</span>
          </button>
        </div>

        {/* Modal Content Body */}
        <div className="modal-body">
          {/* TAB 1: General Preferences */}
          {activeTab === "general" && (
            <>
              <div className="setting-row">
                <div className="setting-info">
                  <span className="setting-title">开机自启</span>
                  <span className="setting-desc">Windows 启动后自动在后台托盘守护</span>
                </div>
                <label className="switch">
                  <input
                    type="checkbox"
                    checked={startupEnabled}
                    onChange={() => onInvoke("set_startup_enabled_cmd", { enabled: !startupEnabled })}
                  />
                  <span className="slider" />
                </label>
              </div>

              <div className="setting-row">
                <div className="setting-info">
                  <span className="setting-title">自动重连守护</span>
                  <span className="setting-desc">网络断开后自动尝试重新认证</span>
                </div>
                <label className="switch">
                  <input
                    type="checkbox"
                    checked={form.auto_reconnect}
                    onChange={(e) => updateField("auto_reconnect", e.target.checked)}
                  />
                  <span className="slider" />
                </label>
              </div>

              <div>
                <span className="settings-section-title">定时巡检与重试间隔</span>
                <div className="settings-grid-two">
                  <div className="input-group">
                    <label className="input-label">失败重试 (秒)</label>
                    <input
                      type="number"
                      className="input-field"
                      min={5}
                      max={3600}
                      value={form.retry_seconds}
                      onChange={(e) => updateField("retry_seconds", Number(e.target.value || 15))}
                    />
                  </div>

                  <div className="input-group">
                    <label className="input-label">在线巡检 (秒)</label>
                    <input
                      type="number"
                      className="input-field"
                      min={30}
                      max={3600}
                      value={form.online_check_seconds}
                      onChange={(e) => updateField("online_check_seconds", Number(e.target.value || 60))}
                    />
                  </div>
                </div>
              </div>
            </>
          )}

          {/* TAB 2: Network Diagnostics */}
          {activeTab === "diagnostic" && (
            <>
              {/* Diagnostic Quick Actions */}
              <div className="tools-action-grid">
                <button
                  className="btn-tool"
                  onClick={() => onInvoke("check_status_cmd")}
                  disabled={taskRunning}
                >
                  <SearchCheck size={16} />
                  <span>在线检测</span>
                </button>
                <button
                  className="btn-tool"
                  onClick={() => onInvoke("reconnect_self_test_cmd")}
                  disabled={taskRunning}
                >
                  <RefreshCw size={16} />
                  <span>重连自测</span>
                </button>
                <button
                  className="btn-tool"
                  onClick={() => onInvoke("diagnose_cmd")}
                  disabled={taskRunning}
                >
                  <Bug size={16} />
                  <span>重新诊断</span>
                </button>
              </div>

              {diagnostic ? (
                <>
                  <div className="diag-conclusion-card">
                    <span
                      className={`diag-status-dot ${diagnostic.online ? "online" : diagnostic.conclusion ? "warning" : "offline"}`}
                    />
                    <div className="diag-conclusion-text">
                      <strong>{diagnostic.conclusion || "诊断完成"}</strong>
                      <span>{diagnostic.radUserInfo || "状态已更新"}</span>
                    </div>
                  </div>

                  <div className="diag-grid">
                    <div className="diag-grid-item">
                      <span>Portal 地址</span>
                      <strong>{diagnostic.portal || "-"}</strong>
                    </div>
                    <div className="diag-grid-item">
                      <span>ac_id</span>
                      <strong>{diagnostic.acId || "-"}</strong>
                    </div>
                    <div className="diag-grid-item">
                      <span>登录出口网卡</span>
                      <strong>{diagnostic.loginOutlet || "-"}</strong>
                    </div>
                    <div className="diag-grid-item">
                      <span>客户端 IP</span>
                      <strong>{diagnostic.currentIp || diagnostic.loginIp || "-"}</strong>
                    </div>
                  </div>

                  {diagnostic.networkPath && (
                    <div className="diag-grid-item">
                      <span>网络路径特征</span>
                      <strong>{diagnostic.networkPath}</strong>
                    </div>
                  )}

                  <div className={`diag-challenge-banner ${challengeOk ? "ok" : "bad"}`}>
                    <span>Challenge 握手状态</span>
                    <span>{challengeOk ? "握手正常 ✓" : "握手异常 ✕"}</span>
                  </div>

                  {diagnostic.probes.length > 0 && (
                    <details className="diag-probes-details">
                      <summary>查看探测明细 ({diagnostic.probes.length} 项)</summary>
                      <div className="diag-probes-content">
                        {diagnostic.probes.map((p, idx) => (
                          <code key={idx}>{p.replace(/^- /, "")}</code>
                        ))}
                      </div>
                    </details>
                  )}
                </>
              ) : (
                <div className="diag-conclusion-card">
                  <span className="diag-status-dot warning" />
                  <div className="diag-conclusion-text">
                    <strong>诊断就绪</strong>
                    <span>点击上方按钮即可发起网络与认证探测</span>
                  </div>
                </div>
              )}
            </>
          )}

          {/* TAB 3: Advanced Network Interface */}
          {activeTab === "advanced" && (
            <>
              <div className="network-outlet-box">
                <div className="network-outlet-header">
                  <span className="settings-section-title" style={{ margin: 0 }}>
                    登录出口网卡
                  </span>
                  <button
                    className="btn-secondary"
                    style={{ padding: "3px 8px", fontSize: "11px" }}
                    onClick={onRefreshInterfaces}
                    disabled={interfacesLoading}
                  >
                    <RefreshCw size={11} className={interfacesLoading ? "spinner" : ""} />
                    <span>刷新</span>
                  </button>
                </div>

                <select
                  className="network-outlet-select"
                  value={selectedIp}
                  onChange={(e) => updateField("bind_ip", e.target.value)}
                >
                  <option value="">自动选择（推荐）</option>
                  {visibleInterfaces.map((iface) => (
                    <option key={`${iface.interface_alias}-${iface.ip}`} value={iface.ip}>
                      {iface.interface_alias} / {iface.ip} {iface.is_likely_campus ? " (可能是校园网)" : ""}
                    </option>
                  ))}
                </select>

                <div style={{ display: "flex", justifyContent: "space-between", alignItems: "center" }}>
                  <label style={{ fontSize: "11px", color: "var(--text-muted)", cursor: "pointer" }}>
                    <input
                      type="checkbox"
                      checked={showAllIfaces}
                      onChange={(e) => setShowAllIfaces(e.target.checked)}
                      style={{ marginRight: 5 }}
                    />
                    显示全部接口
                  </label>

                  {selectedIface && (
                    <span className={`adapter-tag ${selectedIface.is_likely_campus ? "good" : "warn"}`}>
                      {selectedIface.recommendation || selectedIface.interface_alias}
                    </span>
                  )}
                </div>

                {interfacesError && <div style={{ fontSize: "11px", color: "var(--danger)" }}>{interfacesError}</div>}
              </div>

              <div>
                <div style={{ display: "flex", justifyContent: "space-between", alignItems: "center", marginBottom: 6 }}>
                  <span className="settings-section-title" style={{ margin: 0 }}>
                    认证与探测参数
                  </span>
                  <button
                    className="btn-secondary"
                    style={{ padding: "3px 8px", fontSize: "11px" }}
                    onClick={() => onInvoke("detect_portal_cmd", {}, "Portal 探测完成")}
                    disabled={taskRunning}
                  >
                    <SearchCheck size={11} />
                    <span>自动探测</span>
                  </button>
                </div>

                <div className="settings-grid-two">
                  <div className="input-group">
                    <label className="input-label">Portal 地址</label>
                    <input
                      type="text"
                      className="input-field"
                      value={form.portal_url}
                      placeholder="http://172.16.1.3"
                      onChange={(e) => updateField("portal_url", e.target.value)}
                    />
                  </div>

                  <div className="input-group">
                    <label className="input-label">ac_id</label>
                    <input
                      type="text"
                      className="input-field"
                      value={form.ac_id}
                      placeholder="例如: 1"
                      onChange={(e) => updateField("ac_id", e.target.value)}
                    />
                  </div>
                </div>

                <div className="settings-grid-two">
                  <div className="input-group">
                    <label className="input-label">探测连通性地址</label>
                    <input
                      type="text"
                      className="input-field"
                      value={form.probe_url}
                      onChange={(e) => updateField("probe_url", e.target.value)}
                    />
                  </div>

                  <div className="input-group">
                    <label className="input-label">客户端 IP</label>
                    <input
                      type="text"
                      className="input-field"
                      value={form.user_ip}
                      placeholder="留空自动获取"
                      onChange={(e) => updateField("user_ip", e.target.value)}
                    />
                  </div>
                </div>
              </div>
            </>
          )}
        </div>

        {/* Modal Action Buttons */}
        <div className="modal-footer">
          <button
            className="btn-secondary"
            onClick={() => {
              onInvoke("save_config_cmd");
              onClose();
            }}
          >
            <Save size={13} />
            <span>保存并关闭</span>
          </button>
        </div>
      </div>
    </div>
  );
}

// Software Update Modal Component
function UpdateNoticeModal({ currentVersion, update, updating, onInstall, onLater }) {
  return (
    <div className="modal-backdrop">
      <div className="modal-card">
        <div className="modal-header">
          <div className="modal-title-group">
            <span className="modal-title-icon">
              <RefreshCw size={16} />
            </span>
            <span className="modal-title">发现新版本</span>
          </div>
        </div>

        <div className="modal-body">
          <div className="diag-grid">
            <div className="diag-grid-item">
              <span>当前版本</span>
              <strong>v{currentVersion}</strong>
            </div>
            <div className="diag-grid-item">
              <span>最新版本</span>
              <strong>v{update?.version}</strong>
            </div>
          </div>

          <div className="diag-grid-item">
            <span>更新说明</span>
            <div style={{ fontSize: "12px", marginTop: 4, color: "var(--text-primary)", lineHeight: 1.4 }}>
              {update?.body || update?.notes || "包含界面精简重构与连接稳定性提升。"}
            </div>
          </div>
        </div>

        <div className="modal-footer">
          <button className="btn-secondary" onClick={onLater} disabled={updating}>
            稍后再说
          </button>
          <button className="btn-primary" style={{ width: "auto", padding: "8px 16px" }} onClick={onInstall} disabled={updating}>
            {updating ? "更新中..." : "立即更新"}
          </button>
        </div>
      </div>
    </div>
  );
}

// Helper to Parse Diagnostics Plain Text Output
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
    radUserInfo: pick("rad_user_info"),
    challenge: challengeMatch?.[1]?.trim() || "",
    probes: probeLines,
    online: /rad_user_info：online/i.test(text) || /已在线/.test(text),
  };
}

createRoot(document.getElementById("root")).render(<App />);
