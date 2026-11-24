#!/usr/bin/env node
/**
 * H-OpenConnect AppGallery Light-Theme Promotional Poster Generator
 * Outputs 15 exact-dimension PNGs:
 *   - Phone: 5 x 1080x1920 (9:16)
 *   - Pad:   5 x 1920x1280 (3:2)
 *   - PC:    5 x 1920x1080 (16:9)
 */
import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import { fileURLToPath } from "node:url";
import { execFileSync } from "node:child_process";

const __dirname = path.dirname(fileURLToPath(import.meta.url));
const REPO_ROOT = path.resolve(__dirname, "..");
const OUT_ROOT = path.join(REPO_ROOT, "docs/design/screenshots");
const CHROME =
  process.env.CHROME_PATH ||
  "/Applications/Google Chrome.app/Contents/MacOS/Google Chrome";
const TMP_HTML = fs.mkdtempSync(path.join(os.tmpdir(), "hopenconnect-posters-"));
for (const dir of ["phone", "pad", "pc"]) {
  fs.mkdirSync(path.join(OUT_ROOT, dir), { recursive: true });
}

// Canonical Proposal A (Concentric Portal) SVG glyph on 24x24 grid
function brandGlyph(size = 24, color = "#4a840a", strokeWidth = 1.95) {
  return `<svg width="${size}" height="${size}" viewBox="0 0 24 24" fill="none" stroke="${color}" stroke-width="${strokeWidth}" stroke-linecap="round" stroke-linejoin="round">
    <path d="M 18.39 6.47 A 8.6 8.6 0 1 0 18.39 17.53"/>
    <path d="M 15.71 8.72 A 5.1 5.1 0 1 0 15.71 15.28"/>
    <circle cx="11.8" cy="12" r="1.65"/>
    <path d="M 13.45 12 H 20.6"/>
  </svg>`;
}

function appIconTile(size = 44, radius = 12, glyphSize = 26) {
  return `<div style="width:${size}px;height:${size}px;border-radius:${radius}px;background:linear-gradient(180deg,#f8f9fb 0%,#f3f4f7 52%,#eceef2 100%);border:1px solid #d5d7dc;box-shadow:inset 0 1px 0 #fff,0 1px 2px rgba(16,19,25,.05);display:flex;align-items:center;justify-content:center;flex-shrink:0;">
    ${brandGlyph(glyphSize, "#4a840a", 1.95)}
  </div>`;
}

// Crisp Lucide-style 24x24 UI icons matching ui/nsh/src/ui/icons.tsx
function icon(name, size = 18, color = "currentColor", sw = 1.9) {
  const paths = {
    "shield-check": `<path d="M20 13c0 5-3.5 7.5-7.66 8.95a1 1 0 0 1-.67-.01C7.5 20.5 4 18 4 13V6a1 1 0 0 1 1-1c2 0 4.5-1.2 6.24-2.72a1.17 1.17 0 0 1 1.52 0C14.51 3.81 17 5 19 5a1 1 0 0 1 1 1z"/><path d="m9 12 2 2 4-4"/>`,
    server: `<rect width="20" height="8" x="2" y="2" rx="2" ry="2"/><rect width="20" height="8" x="2" y="14" rx="2" ry="2"/><line x1="6" x2="6.01" y1="6" y2="6"/><line x1="6" x2="6.01" y1="18" y2="18"/>`,
    activity: `<path d="M22 12h-2.48a2 2 0 0 0-1.93 1.46l-2.35 8.36a.25.25 0 0 1-.48 0L9.24 2.18a.25.25 0 0 0-.48 0l-2.35 8.36A2 2 0 0 1 4.49 12H2"/>`,
    settings: `<path d="M12.22 2h-.44a2 2 0 0 0-2 2v.18a2 2 0 0 1-1 1.73l-.43.25a2 2 0 0 1-2 0l-.15-.08a2 2 0 0 0-2.73.73l-.22.38a2 2 0 0 0 .73 2.73l.15.1a2 2 0 0 1 1 1.72v.51a2 2 0 0 1-1 1.74l-.15.09a2 2 0 0 0-.73 2.73l.22.38a2 2 0 0 0 2.73.73l.15-.08a2 2 0 0 1 2 0l.43.25a2 2 0 0 1 1 1.73V20a2 2 0 0 0 2 2h.44a2 2 0 0 0 2-2v-.18a2 2 0 0 1 1-1.73l.43-.25a2 2 0 0 1 2 0l.15.08a2 2 0 0 0 2.73-.73l.22-.39a2 2 0 0 0-.73-2.73l-.15-.08a2 2 0 0 1-1-1.74v-.5a2 2 0 0 1 1-1.74l.15-.09a2 2 0 0 0 .73-2.73l-.22-.38a2 2 0 0 0-2.73-.73l-.15.08a2 2 0 0 1-2 0l-.43-.25a2 2 0 0 1-1-1.73V4a2 2 0 0 0-2-2z"/><circle cx="12" cy="12" r="3"/>`,
    "file-text": `<path d="M15 2H6a2 2 0 0 0-2 2v16a2 2 0 0 0 2 2h12a2 2 0 0 0 2-2V7Z"/><path d="M14 2v4a2 2 0 0 0 2 2h4"/><path d="M10 9H8"/><path d="M16 13H8"/><path d="M16 17H8"/>`,
    power: `<path d="M12 2v10"/><path d="M18.4 6.6a9 9 0 1 1-12.77.04"/>`,
    stop: `<rect x="5" y="5" width="14" height="14" rx="2.5" fill="${color}" stroke="none"/>`,
    plus: `<path d="M5 12h14"/><path d="M12 5v14"/>`,
    "scan-line": `<path d="M3 7V5a2 2 0 0 1 2-2h2"/><path d="M17 3h2a2 2 0 0 1 2 2v2"/><path d="M21 17v2a2 2 0 0 1-2 2h-2"/><path d="M7 21H5a2 2 0 0 1-2-2v-2"/><path d="M7 12h10"/>`,
    search: `<circle cx="11" cy="11" r="8"/><path d="m21 21-4.3-4.3"/>`,
    star: `<polygon points="12 2 15.09 8.26 22 9.27 17 14.14 18.18 21.02 12 17.77 5.82 21.02 7 14.14 2 9.27 8.91 8.26 12 2"/>`,
    "star-filled": `<polygon points="12 2 15.09 8.26 22 9.27 17 14.14 18.18 21.02 12 17.77 5.82 21.02 7 14.14 2 9.27 8.91 8.26 12 2" fill="${color}"/>`,
    ellipsis: `<circle cx="12" cy="12" r="1"/><circle cx="19" cy="12" r="1"/><circle cx="5" cy="12" r="1"/>`,
    "panel-left": `<rect width="18" height="18" x="3" y="3" rx="2"/><path d="M9 3v18"/>`,
    "refresh-cw": `<path d="M3 12a9 9 0 0 1 9-9 9.75 9.75 0 0 1 6.74 2.74L21 8"/><path d="M21 3v5h-5"/><path d="M21 12a9 9 0 0 1-9 9 9.75 9.75 0 0 1-6.74-2.74L3 16"/><path d="M8 16H3v5"/>`,
    "arrow-up": `<path d="m5 12 7-7 7 7"/><path d="M12 19V5"/>`,
    "arrow-down": `<path d="M12 5v14"/><path d="m19 12-7 7-7-7"/>`,
    "arrow-left": `<path d="m12 19-7-7 7-7"/><path d="M19 12H5"/>`,
    "chevron-down": `<path d="m6 9 6 6 6-6"/>`,
    "chevron-right": `<path d="m9 18 6-6-6-6"/>`,
    check: `<path d="M20 6 9 17l-5-5"/>`,
    "qr-code": `<rect width="5" height="5" x="3" y="3" rx="1"/><rect width="5" height="5" x="16" y="3" rx="1"/><rect width="5" height="5" x="3" y="16" rx="1"/><path d="M21 16h-3a2 2 0 0 0-2 2v3"/><path d="M21 21v.01"/><path d="M12 7v3a2 2 0 0 1-2 2H7"/><path d="M3 12h.01"/><path d="M12 3h.01"/><path d="M12 16v.01"/><path d="M16 12h1"/><path d="M21 12v.01"/><path d="M12 21v-1"/>`,
    "file-pen": `<path d="M12 22h6a2 2 0 0 0 2-2V7l-5-5H6a2 2 0 0 0-2 2v10"/><path d="M14 2v4a2 2 0 0 0 2 2h4"/><path d="M10.4 12.6a2 2 0 1 1 3 3L8 21l-4 1 1-4Z"/>`,
    trash: `<path d="M3 6h18"/><path d="M19 6v14c0 1-1 2-2 2H7c-1 0-2-1-2-2V6"/><path d="M8 6V4c0-1 1-2 2-2h4c1 0 2 1 2 2v2"/>`,
    copy: `<rect width="14" height="14" x="8" y="8" rx="2" ry="2"/><path d="M4 16c-1.1 0-2-.9-2-2V4c0-1.1.9-2 2-2h10c1.1 0 2 .9 2 2"/>`,
    x: `<path d="M18 6 6 18"/><path d="m6 6 12 12"/>`,
    lock: `<rect width="18" height="11" x="3" y="11" rx="2" ry="2"/><path d="M7 11V7a5 5 0 0 1 10 0v4"/>`,
    key: `<path d="m15.5 7.5 2.3 2.3a1 1 0 0 0 1.4 0l2.1-2.1a1 1 0 0 0 0-1.4L19 4"/><path d="m21 2-9.6 9.6"/><circle cx="7.5" cy="15.5" r="5.5"/>`,
    globe: `<circle cx="12" cy="12" r="10"/><path d="M12 2a14.5 14.5 0 0 0 0 20 14.5 14.5 0 0 0 0-20"/><path d="M2 12h20"/>`,
    wifi: `<path d="M12 20h.01"/><path d="M2 8.82a15 15 0 0 1 20 0"/><path d="M5 12.859a10 10 0 0 1 14 0"/><path d="M8.5 16.429a5 5 0 0 1 7 0"/>`,
    play: `<polygon points="6 3 20 12 6 21 6 3"/>`,
    download: `<path d="M21 15v4a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2v-4"/><polyline points="7 10 12 15 17 10"/><line x1="12" x2="12" y1="15" y2="3"/>`,
    info: `<circle cx="12" cy="12" r="10"/><path d="M12 16v-4"/><path d="M12 8h.01"/>`,
  };
  return `<svg width="${size}" height="${size}" viewBox="0 0 24 24" fill="none" stroke="${color}" stroke-width="${sw}" stroke-linecap="round" stroke-linejoin="round" style="flex-shrink:0;display:block;">${paths[name] || ""}</svg>`;
}

// ============================================================================
// Shared UI building blocks (100% faithful to ui/nsh/src/)
// ============================================================================

const PROFILES = [
  {
    id: "hq",
    name: "公司总部网关",
    server: "gw.corp.example.com",
    group: "研发部门",
    username: "chen.wei",
    method: "账号密码 + 动态令牌",
    routing: "自动分流",
    reconnect: "已开启",
    favorite: true,
    active: true,
  },
  {
    id: "sg",
    name: "新加坡研发中心",
    server: "sg-edge.example.com:8443",
    group: "新加坡集群",
    username: "chen.wei",
    method: "网页单点登录 (SAML)",
    routing: "全局路由",
    reconnect: "已开启",
    favorite: true,
    active: false,
  },
  {
    id: "eu",
    name: "法兰克福办公接入",
    server: "eu-west.example.com",
    group: "欧洲分支",
    username: "c.wei@example.com",
    method: "客户端证书",
    routing: "自动分流",
    reconnect: "已关闭",
    favorite: false,
    active: false,
  },
  {
    id: "jp",
    name: "东京测试集群",
    server: "jp-lab.example.com",
    group: "默认",
    username: "ops-admin",
    method: "账号密码",
    routing: "自动分流",
    reconnect: "已关闭",
    favorite: false,
    active: false,
  },
];

const LOGS = [
  {
    ts: "09:40:12",
    full: "2026-11-24 09:40:12",
    level: "info",
    label: "信息",
    module: "连接服务",
    target: "gw.corp.example.com",
    msg: "Session engine ready, loading profile gw.corp.example.com",
  },
  {
    ts: "09:40:14",
    full: "2026-11-24 09:40:14",
    level: "info",
    label: "信息",
    module: "身份验证",
    target: "gw.corp.example.com",
    msg: "Interactive MFA challenge verified for group 研发部门",
  },
  {
    ts: "09:40:15",
    full: "2026-11-24 09:40:15",
    level: "info",
    label: "信息",
    module: "加密传输",
    target: "gw.corp.example.com:443",
    msg: "DTLS 1.2 cipher AES-256-GCM negotiated, MTU 1400",
  },
  {
    ts: "09:40:16",
    full: "2026-11-24 09:40:16",
    level: "info",
    label: "信息",
    module: "网络隧道",
    target: "10.24.0.15",
    msg: "Tunnel established: IPv4 10.24.0.15/24, split routes 12",
  },
  {
    ts: "09:40:48",
    full: "2026-11-24 09:40:48",
    level: "warn",
    label: "警告",
    module: "加密传输",
    target: "gw.corp.example.com",
    msg: "DPD probe latency elevated (68 ms), keeping DTLS session",
  },
  {
    ts: "09:41:02",
    full: "2026-11-24 09:41:02",
    level: "info",
    label: "信息",
    module: "DNS",
    target: "10.24.0.1",
    msg: "Split DNS active for corp.example.com via 10.24.0.1",
  },
  {
    ts: "09:41:18",
    full: "2026-11-24 09:41:18",
    level: "info",
    label: "信息",
    module: "网络隧道",
    target: "10.0.0.0/8",
    msg: "Route table synchronized: 12 corporate subnets, local LAN bypassed",
  },
  {
    ts: "09:41:35",
    full: "2026-11-24 09:41:35",
    level: "debug",
    label: "调试",
    module: "加密传输",
    target: "gw.corp.example.com",
    msg: "Keepalive RTT 18 ms · RX 284.6 MB / TX 42.1 MB · 0 drops",
  },
];

const TRAFFIC_BARS = [
  [24, 10],
  [36, 14],
  [30, 12],
  [48, 18],
  [62, 22],
  [54, 19],
  [70, 26],
  [65, 21],
  [84, 30],
  [76, 25],
  [58, 18],
  [68, 22],
  [92, 34],
  [86, 28],
  [74, 24],
  [80, 27],
  [95, 36],
  [88, 31],
  [72, 24],
  [85, 29],
];

function badge(label, tone = "neutral") {
  const map = {
    success: ["var(--up)", "var(--up-tint)"],
    brand: ["var(--brand)", "var(--brand-tint)"],
    info: ["var(--info)", "var(--info-tint)"],
    warning: ["var(--warn)", "var(--warn-tint)"],
    danger: ["var(--danger)", "var(--danger-tint)"],
    neutral: ["var(--muted)", "var(--muted-bg)"],
  };
  const [fg, bg] = map[tone] || map.neutral;
  return `<span class="badge" style="color:${fg};background:${bg};">${label}</span>`;
}

function toggleSwitch(on = true) {
  return `<div class="toggle ${on ? "on" : ""}"><div class="toggle-knob"></div></div>`;
}

function speedGrid(up = "3.2 MB/s", down = "18.6 MB/s", compact = false) {
  return `<div class="speed-grid ${compact ? "compact" : ""}">
    <div class="speed-cell">
      <div class="speed-head">${icon("arrow-up", compact ? 13 : 15, "#3d63d1", 2.1)}<span>上传</span></div>
      <div class="speed-val">${up}</div>
    </div>
    <div class="speed-sep"></div>
    <div class="speed-cell">
      <div class="speed-head">${icon("arrow-down", compact ? 13 : 15, "#138a5a", 2.1)}<span>下载</span></div>
      <div class="speed-val">${down}</div>
    </div>
  </div>`;
}

function trafficChart(height = 136) {
  const bars = TRAFFIC_BARS.map(
    ([d, u]) => `<div class="chart-col">
      <div class="chart-bar down" style="height:${d}%;"></div>
      <div class="chart-bar up" style="height:${u}%;"></div>
    </div>`,
  ).join("");
  return `<div class="chart-box">
    <div class="chart-plot" style="height:${height}px;">${bars}</div>
    <div class="chart-legend">
      <div class="legend-item"><span class="legend-dot" style="background:#3d63d1"></span>下载峰值 21.4 MB/s</div>
      <div class="legend-item"><span class="legend-dot" style="background:#138a5a"></span>上传峰值 5.8 MB/s</div>
    </div>
  </div>`;
}

function detailRow(label, value, last = false, toneColor = "") {
  return `<div class="detail-row ${last ? "last" : ""}">
    <span class="detail-k">${label}</span>
    <span class="detail-v" ${toneColor ? `style="color:${toneColor}"` : ""}>${value}</span>
  </div>`;
}

// ============================================================================
// Inner Screen Renderers (Phone / Pad / PC faithful to ui/nsh/src/)
// ============================================================================

function phoneStatusBar() {
  return `<div class="phone-status">
    <span class="st-time">09:41</span>
    <div class="st-pill"></div>
    <div class="st-right">
      ${icon("wifi", 14, "#101319", 2.2)}
      <div class="st-bat"><div class="st-bat-fill"></div></div>
    </div>
  </div>`;
}

function phoneBottomNav(active) {
  const items = [
    { id: "home", label: "概览", ic: "shield-check" },
    { id: "profiles", label: "配置", ic: "server" },
    { id: "statistics", label: "统计", ic: "activity" },
    { id: "settings", label: "设置", ic: "settings" },
  ];
  return `<div class="phone-bottom">
    <div class="phone-tabs">
      ${items
        .map(
          (it) => `<div class="phone-tab ${it.id === active ? "active" : ""}">
        ${icon(it.ic, 20, it.id === active ? "#4a840a" : "#5f636a", it.id === active ? 2.15 : 1.85)}
        <span>${it.label}</span>
      </div>`,
        )
        .join("")}
    </div>
    <div class="phone-home-bar"></div>
  </div>`;
}

function sidebarHtml(active, isDesktop, connected = true) {
  const navs = [
    { id: "home", label: "概览", ic: "shield-check", key: "Ctrl+1" },
    { id: "profiles", label: "配置", ic: "server", key: "Ctrl+2" },
    { id: "statistics", label: "统计", ic: "activity", key: "Ctrl+3" },
    { id: "logs", label: "日志", ic: "file-text", key: "Ctrl+4" },
  ];
  const itemHtml = (it) => `<div class="sb-item ${it.id === active ? "active" : ""}">
    ${icon(it.ic, isDesktop ? 16 : 18, it.id === active ? "#101319" : "#5f636a", it.id === active ? 2.1 : 1.85)}
    <span class="sb-label">${it.label}</span>
    ${isDesktop ? `<span class="sb-key">${it.key}</span>` : ""}
  </div>`;
  return `<div class="sidebar ${isDesktop ? "desktop" : "pad"}">
    <div class="sb-top">
      <div class="sb-brand">
        ${appIconTile(26, 8, 17)}
        <span class="sb-brand-name">H-OpenConnect</span>
        ${icon("panel-left", 16, "#5f636a", 1.9)}
      </div>
    </div>
    <div class="sb-nav">
      <div class="sb-group">${navs.map(itemHtml).join("")}</div>
      <div class="sb-spacer"></div>
      <div class="sb-sep"></div>
      ${itemHtml({ id: "settings", label: "设置", ic: "settings", key: "Ctrl+," })}
    </div>
    <div class="sb-foot">
      <div class="sb-status"><span class="${connected ? "dot-up" : "dot-idle"}"></span><span>${connected ? "已连接" : "正在验证"}</span></div>
      <span class="sb-ver">v1.0.1</span>
    </div>
  </div>`;
}

function pcWindowControls() {
  return `<div class="win-controls">
    <div class="win-btn"><svg width="11" height="11" viewBox="0 0 12 12"><line x1="2" y1="6" x2="10" y2="6" stroke="#3c4048" stroke-width="1.3"/></svg></div>
    <div class="win-btn"><svg width="11" height="11" viewBox="0 0 12 12"><rect x="2.2" y="2.2" width="7.6" height="7.6" rx="1.2" fill="none" stroke="#3c4048" stroke-width="1.2"/></svg></div>
    <div class="win-btn"><svg width="11" height="11" viewBox="0 0 12 12"><path d="M2.6 2.6 L9.4 9.4 M9.4 2.6 L2.6 9.4" stroke="#3c4048" stroke-width="1.3" stroke-linecap="round"/></svg></div>
  </div>`;
}

function heroCard({ connected = true, authenticating = false, compact = false }) {
  const stBadge = authenticating
    ? badge("正在验证身份", "info")
    : connected
      ? badge("已连接", "success")
      : badge("未连接", "neutral");
  const btn = authenticating
    ? `<div class="btn primary full"><span class="spin"></span><span>正在验证身份…</span></div>`
    : connected
      ? `<div class="btn stop full">${icon("stop", 14, "#ffffff", 2)}<span>断开连接</span></div>`
      : `<div class="btn primary full">${icon("power", 16, "#13160c", 2.1)}<span>立即连接</span></div>`;
  return `<div class="card">
    <div class="hero-top">
      <div class="hero-icon ${connected ? "connected" : ""}">
        ${brandGlyph(26, connected ? "#13160c" : "#4a840a", 2.0)}
      </div>
      <div class="hero-meta">
        <div class="hero-title-row">
          <span class="hero-title">公司总部网关</span>
          ${stBadge}
        </div>
        <div class="hero-sub">gw.corp.example.com</div>
      </div>
    </div>
    ${btn}
    <div class="sep"></div>
    <div class="toggle-row">
      <div class="toggle-copy">
        <div class="toggle-label">全局路由</div>
        ${compact ? "" : `<div class="toggle-desc">让所有流量通过当前安全连接</div>`}
      </div>
      ${toggleSwitch(false)}
    </div>
  </div>`;
}

function sessionCard() {
  return `<div class="card">
    <div class="card-head">
      <span class="card-title">当前会话</span>
      <span class="link-act">查看统计</span>
    </div>
    ${detailRow("连接时长", "01:42:18")}
    ${detailRow("分配地址", "10.24.0.15")}
    ${detailRow("网关", "gw.corp.example.com")}
    ${detailRow("累计流量", "284.6 MB ↓ · 42.1 MB ↑", true)}
  </div>`;
}

function quickProfilesList(count = 4, showNewBtn = true, connected = true) {
  const rows = PROFILES.slice(0, count)
    .map(
      (p, i) => `<div class="list-row ${p.active ? "selected" : ""} ${i === count - 1 ? "last" : ""}">
      <div class="icon-tile">${icon(p.favorite ? "star-filled" : "server", 17, p.favorite ? "#4a840a" : "#5f636a", 1.9)}</div>
      <div class="row-body">
        <div class="row-title-line">
          <span class="row-title">${p.name}</span>
          ${p.active ? badge(connected ? "已连接" : "当前", connected ? "success" : "brand") : ""}
        </div>
        <div class="row-sub">${p.server}</div>
      </div>
    </div>`,
    )
    .join("");
  return `<div class="sec">
    <div class="sec-head">
      <span class="sec-title">连接配置</span>
      ${showNewBtn ? `<div class="btn secondary sm">${icon("plus", 13, "#101319", 2.2)}<span>新建</span></div>` : ""}
    </div>
    <div class="list-surface">${rows}</div>
  </div>`;
}

function profileListPane(selectedId = "hq", isDesktop = false, showContextMenu = false) {
  const rows = PROFILES.map(
    (p, i) => `<div class="list-row ${p.id === selectedId ? "selected" : ""} ${i === PROFILES.length - 1 ? "last" : ""}" style="position:relative;">
      <div class="icon-tile">${icon(p.favorite ? "star-filled" : "server", 16, p.favorite ? "#4a840a" : "#5f636a", 1.9)}</div>
      <div class="row-body">
        <div class="row-title-line">
          <span class="row-title">${p.name}</span>
          ${p.active ? badge("已连接", "success") : ""}
        </div>
        <div class="row-sub">${p.server}</div>
      </div>
      <div class="row-actions">
        ${icon(p.favorite ? "star-filled" : "star", 16, p.favorite ? "#4a840a" : "#9a9da3", 1.9)}
        ${icon("ellipsis", 16, "#5f636a", 2)}
      </div>
    </div>`,
  ).join("");
  const menuOverlay = showContextMenu
    ? `<div class="ctx-menu">
        <div class="ctx-item">${icon("check", 14, "#5f636a", 2)}<span>设为当前连接</span></div>
        <div class="ctx-item active">${icon("file-pen", 14, "#5f636a", 2)}<span>编辑配置</span></div>
        <div class="ctx-item">${icon("star-filled", 14, "#4a840a", 1.9)}<span>取消收藏</span></div>
        <div class="ctx-item">${icon("qr-code", 14, "#5f636a", 2)}<span>导出二维码…</span></div>
        <div class="ctx-sep"></div>
        <div class="ctx-item danger">${icon("trash", 14, "#d43a4a", 2)}<span>删除配置…</span></div>
      </div>`
    : "";
  return `<div class="sec" style="gap:12px;position:relative;">
    <div class="filter-bar">
      <div class="seg ${isDesktop ? "inline" : ""}">
        <div class="seg-item active">全部 <span class="seg-cnt">4</span></div>
        <div class="seg-item">收藏 <span class="seg-cnt">2</span></div>
      </div>
      <div class="search-field">${icon("search", 15, "#9a9da3", 2)}<span>搜索名称或服务器</span></div>
    </div>
    <div class="list-surface">${rows}</div>
    ${menuOverlay}
  </div>`;
}

function profileDetailCard(p = PROFILES[0]) {
  return `<div class="card">
    <div class="card-head">
      <div>
        <div class="card-title" style="font-size:16px;">${p.name}</div>
        <div class="hero-sub" style="margin-top:2px;">${p.server}</div>
      </div>
      ${p.active ? badge("已连接", "success") : badge("已收藏", "brand")}
    </div>
    ${detailRow("认证方式", p.method)}
    ${detailRow("认证分组", p.group)}
    ${detailRow("用户名", p.username)}
    ${detailRow("路由策略", p.routing)}
    ${detailRow("自动重连", p.reconnect, true)}
    <div class="sep"></div>
    <div class="actions-row">
      ${p.active ? "" : `<div class="btn primary sm">${icon("check", 13, "#13160c", 2.2)}<span>设为当前</span></div>`}
      <div class="btn secondary sm">${icon("file-pen", 13, "#101319", 2)}<span>编辑</span></div>
      <div class="btn secondary sm">${icon("qr-code", 13, "#101319", 2)}<span>二维码</span></div>
      <div class="btn danger-outline sm">${icon("trash", 13, "#d43a4a", 2)}<span>删除</span></div>
    </div>
  </div>`;
}

function authChallengeDialog(isSheet = false) {
  return `<div class="modal-scrim ${isSheet ? "sheet-align" : "dialog-align"}">
    <div class="${isSheet ? "sheet-card" : "dialog-card"}">
      ${isSheet ? `<div class="sheet-grabber"></div>` : ""}
      <div class="modal-head">
        <div>
          <div class="modal-title">身份验证</div>
          <div class="modal-sub">gw.corp.example.com · 企业统一认证</div>
        </div>
        <div class="icon-btn-sm">${icon("x", 16, "#5f636a", 2)}</div>
      </div>
      <div class="modal-body">
        <div class="banner-box">
          ${icon("shield-check", 16, "#4a840a", 2)}
          <span>请输入企业身份验证器生成的 6 位动态口令以完成安全隧道建立。</span>
        </div>
        <div class="field-group">
          <div class="field-label">认证分组</div>
          <div class="select-box"><span>研发部门 (R&amp;D-SSO)</span>${icon("chevron-down", 15, "#5f636a", 2)}</div>
        </div>
        <div class="field-group">
          <div class="field-label">动态验证码</div>
          <div class="input-box focused"><span>482 916</span><span class="caret"></span></div>
        </div>
        <div class="auth-methods-hint">
          <span class="pill-chip">${icon("lock", 12, "#4a840a", 2)}客户端证书</span>
          <span class="pill-chip">${icon("globe", 12, "#3d63d1", 2)}网页单点登录</span>
          <span class="pill-chip">${icon("key", 12, "#138a5a", 2)}TOTP 令牌</span>
        </div>
      </div>
      <div class="modal-foot">
        <div class="btn secondary" style="flex:1">取消</div>
        <div class="btn primary" style="flex:1.5">${icon("check", 15, "#13160c", 2.2)}<span>提交验证</span></div>
      </div>
    </div>
  </div>`;
}

function networkCard() {
  return `<div class="card">
    <div class="card-head">
      <span class="card-title">网络信息</span>
      ${badge("DTLS 已启用", "success")}
    </div>
    ${detailRow("IPv4", "10.24.0.15 / 255.255.255.0")}
    ${detailRow("IPv6", "fd00:24::15 / 64")}
    ${detailRow("网关", "gw.corp.example.com")}
    ${detailRow("MTU", "1400")}
    ${detailRow("DNS", "10.24.0.1, 10.24.0.2")}
    ${detailRow("搜索域", "corp.example.com")}
    ${detailRow("路由规则", "10.0.0.0/8, 172.16.0.0/12", true)}
  </div>`;
}

function trafficCard(chartHeight = 132) {
  return `<div class="card">
    <div class="card-head">
      <span class="card-title">实时速率</span>
      <span class="meta-text">最近 60 秒</span>
    </div>
    ${trafficChart(chartHeight)}
    <div class="sep"></div>
    ${detailRow("已接收", "284.6 MB (218,430)")}
    ${detailRow("已发送", "42.1 MB (64,912)")}
    ${detailRow("连接时长", "01:42:18", true)}
  </div>`;
}

function logStreamRows(selectedIdx = 4, maxRows = 6) {
  return LOGS.slice(0, maxRows)
    .map((l, idx) => {
      const tone =
        l.level === "warn" ? "warning" : l.level === "error" ? "danger" : "info";
      return `<div class="log-row ${idx === selectedIdx ? "selected" : ""}">
        <span class="log-ts">${l.ts}</span>
        ${badge(l.label, tone)}
        <span class="log-msg">${l.msg}</span>
      </div>`;
    })
    .join("");
}

function logInspectorCard(entry = LOGS[4], showClose = true) {
  return `<div class="inspector-pane">
    <div class="sec-head" style="margin-bottom:2px;">
      <span class="sec-title">日志详情</span>
      <div style="display:flex;align-items:center;gap:6px;">
        ${badge(entry.label, "warning")}
        ${showClose ? `<div class="icon-btn-sm">${icon("x", 14, "#5f636a", 2)}</div>` : ""}
      </div>
    </div>
    <div class="code-well">${entry.msg}</div>
    <div class="list-surface" style="padding:10px 14px;">
      ${detailRow("时间", entry.full)}
      ${detailRow("级别", "警告 (WARN)", false, "#b26a00")}
      ${detailRow("模块", entry.module)}
      ${detailRow("关联目标", entry.target, true)}
    </div>
    <div class="actions-row">
      <div class="btn secondary sm">${icon("copy", 13, "#101319", 2)}<span>复制此条日志</span></div>
    </div>
  </div>`;
}

// ============================================================================
// Form-Factor Screen Assemblers
// ============================================================================

function renderPhoneScreen(slide) {
  let title = "概览";
  let headerRight = "";
  let body = "";
  let activeTab = "home";
  let overlay = "";

  if (slide === 1) {
    title = "概览";
    activeTab = "home";
    body = `<div class="p-body">
      ${heroCard({ connected: true })}
      ${speedGrid("3.2 MB/s", "18.6 MB/s")}
      ${sessionCard()}
    </div>`;
  } else if (slide === 2) {
    title = "配置";
    activeTab = "profiles";
    headerRight = `<div class="hdr-acts">
      <div class="btn secondary sm">${icon("scan-line", 14, "#101319", 2)}<span>扫码</span></div>
      <div class="btn primary sm">${icon("plus", 14, "#13160c", 2.2)}<span>新建</span></div>
    </div>`;
    body = `<div class="p-body">
      ${profileListPane("hq", false, false)}
      <div class="qr-promo-card">
        <div class="qr-mini-box">
          ${icon("qr-code", 40, "#101319", 1.7)}
        </div>
        <div class="qr-promo-text">
          <div class="row-title">二维码快速迁移配置</div>
          <div class="row-sub">一键导出高清二维码，扫一扫即刻导入</div>
        </div>
      </div>
    </div>`;
  } else if (slide === 3) {
    title = "概览";
    activeTab = "home";
    body = `<div class="p-body">
      ${heroCard({ connected: false, authenticating: true })}
      ${quickProfilesList(3, true, false)}
    </div>`;
    overlay = authChallengeDialog(true);
  } else if (slide === 4) {
    title = "统计";
    activeTab = "statistics";
    body = `<div class="p-body" style="gap:12px;">
      ${speedGrid("3.2 MB/s", "18.6 MB/s", true)}
      ${trafficCard(86)}
      <div class="card" style="gap:8px;padding:14px 16px;">
        <div class="card-head">
          <span class="card-title">网络信息</span>
          ${badge("DTLS 已启用", "success")}
        </div>
        ${detailRow("IPv4", "10.24.0.15 / 255.255.255.0")}
        ${detailRow("IPv6", "fd00:24::15 / 64")}
        ${detailRow("网关 · MTU", "gw.corp.example.com · 1400")}
        ${detailRow("DNS", "10.24.0.1, 10.24.0.2", true)}
      </div>
    </div>`;
  } else {
    title = "日志";
    activeTab = "settings";
    headerRight = `<div class="hdr-acts">
      <div class="btn stop sm">${icon("stop", 11, "#ffffff", 2)}<span>停止记录</span></div>
      <div class="icon-btn-sm">${icon("trash", 15, "#5f636a", 2)}</div>
    </div>`;
    body = `<div class="p-body" style="gap:10px;flex:1;">
      <div class="seg">
        <div class="seg-item active">当前日志</div>
        <div class="seg-item">历史归档 <span class="seg-cnt">3</span></div>
      </div>
      <div class="status-note">
        <span class="dot-up"></span><span>正在记录并按天保存</span><span class="faint">·</span><span>3 个日志文件</span>
        <span style="flex:1"></span><span>8 条</span>
      </div>
      <div class="search-field">${icon("search", 15, "#9a9da3", 2)}<span>搜索关键词、地址或错误</span></div>
      <div class="seg sm-seg">
        <div class="seg-item active">全部</div>
        <div class="seg-item">信息</div>
        <div class="seg-item">警告</div>
        <div class="seg-item">错误</div>
        <div class="seg-item">调试</div>
      </div>
      <div class="log-surface" style="flex:1;">
        ${logStreamRows(4, 8)}
      </div>
    </div>`;
  }

  return `<div class="app-screen phone-screen">
    ${phoneStatusBar()}
    <div class="app-header">
      ${slide === 5 ? `<div class="icon-btn-sm" style="margin-right:2px;">${icon("arrow-left", 18, "#101319", 2)}</div>` : ""}
      <div class="hdr-title">${title}</div>
      ${headerRight}
    </div>
    <div class="app-main">${body}</div>
    ${slide === 5 ? `<div class="phone-home-only"><div class="phone-home-bar"></div></div>` : phoneBottomNav(activeTab)}
    ${overlay}
  </div>`;
}

function renderWideScreen(slide, isDesktop) {
  let activeNav = "home";
  let title = "概览";
  let headerRight = "";
  let body = "";
  let overlay = "";

  if (slide === 1) {
    activeNav = "home";
    title = "概览";
    headerRight = `<div class="btn secondary sm">${icon("refresh-cw", 13, "#101319", 2)}<span>刷新</span></div>`;
    body = `<div class="w-two-col">
      <div class="w-col-left">
        ${heroCard({ connected: true, compact: isDesktop })}
        <div class="sec">
          <div class="sec-head"><span class="sec-title">速率</span><span class="meta-text">本次 326.7 MB</span></div>
          ${speedGrid("3.2 MB/s", "18.6 MB/s", isDesktop)}
        </div>
        ${sessionCard()}
      </div>
      <div class="w-col-right">
        ${quickProfilesList(4, true)}
      </div>
    </div>`;
  } else if (slide === 2) {
    activeNav = "profiles";
    title = "配置";
    headerRight = isDesktop
      ? `<div class="btn secondary sm">${icon("plus", 13, "#101319", 2.2)}<span>新建连接…</span></div>`
      : `<div class="hdr-acts">
          <div class="btn secondary sm">${icon("scan-line", 13, "#101319", 2)}<span>扫码</span></div>
          <div class="btn primary sm">${icon("plus", 13, "#13160c", 2.2)}<span>新建</span></div>
        </div>`;
    body = `<div class="w-master-detail">
      <div class="w-list-col">${profileListPane(isDesktop ? "sg" : "hq", isDesktop, isDesktop)}</div>
      <div class="w-detail-col">${profileDetailCard(isDesktop ? PROFILES[1] : PROFILES[0])}</div>
    </div>`;
  } else if (slide === 3) {
    activeNav = "home";
    title = "概览";
    headerRight = `<div class="btn secondary sm">${icon("refresh-cw", 13, "#101319", 2)}<span>刷新</span></div>`;
    body = `<div class="w-two-col">
      <div class="w-col-left">
        ${heroCard({ connected: false, authenticating: true, compact: isDesktop })}
        <div class="sec">
          <div class="sec-head"><span class="sec-title">速率</span></div>
          ${speedGrid("—", "—", isDesktop)}
        </div>
      </div>
      <div class="w-col-right">
        ${quickProfilesList(4, true, false)}
      </div>
    </div>`;
    overlay = authChallengeDialog(false);
  } else if (slide === 4) {
    activeNav = "statistics";
    title = "统计";
    headerRight = `<div class="btn secondary sm">${icon("refresh-cw", 13, "#101319", 2)}<span>刷新</span></div>`;
    body = `<div class="w-equal-col">
      <div class="w-eq-col">
        ${speedGrid("3.2 MB/s", "18.6 MB/s", isDesktop)}
        ${trafficCard(isDesktop ? 136 : 156)}
      </div>
      <div class="w-eq-col">
        ${networkCard()}
      </div>
    </div>`;
  } else {
    activeNav = "logs";
    title = "日志";
    headerRight = `<div class="hdr-acts">
      ${badge("正在记录", "brand")}
      <div class="btn secondary sm">${icon("stop", 11, "#101319", 2)}<span>停止记录</span></div>
      <div class="btn secondary sm">${icon("trash", 13, "#101319", 2)}<span>清空…</span></div>
    </div>`;
    if (isDesktop) {
      body = `<div class="logs-pc-wrap">
        <div class="logs-pc-bar">
          <div class="seg inline">
            <div class="seg-item active">当前日志</div>
            <div class="seg-item">历史归档 <span class="seg-cnt">3</span></div>
          </div>
          <div class="search-field" style="flex:1;">${icon("search", 14, "#9a9da3", 2)}<span>搜索关键词、地址或错误</span></div>
        </div>
        <div class="logs-pc-bar">
          <div class="seg inline sm-seg">
            <div class="seg-item active">全部</div>
            <div class="seg-item">信息</div>
            <div class="seg-item">警告</div>
            <div class="seg-item">错误</div>
            <div class="seg-item">调试</div>
          </div>
          <div class="status-note" style="flex:1;">
            <span class="dot-up"></span><span>正在记录并按天保存</span><span class="faint">·</span><span>3 个日志文件</span>
            <span style="flex:1"></span><span>8 条</span>
          </div>
        </div>
        <div class="logs-pc-split">
          <div class="log-surface" style="flex:1;">${logStreamRows(4, 8)}</div>
          <div class="inspector-drawer">${logInspectorCard(LOGS[4], true)}</div>
        </div>
      </div>`;
    } else {
      body = `<div class="logs-pad-split">
        <div class="logs-pad-left">
          <div class="seg">
            <div class="seg-item active">当前日志</div>
            <div class="seg-item">历史归档 <span class="seg-cnt">3</span></div>
          </div>
          <div class="status-note">
            <span class="dot-up"></span><span>正在记录并按天保存</span><span class="faint">·</span><span>3 个日志文件</span>
            <span style="flex:1"></span><span>8 条</span>
          </div>
          <div class="search-field">${icon("search", 15, "#9a9da3", 2)}<span>搜索关键词、地址或错误</span></div>
          <div class="seg sm-seg">
            <div class="seg-item active">全部</div>
            <div class="seg-item">信息</div>
            <div class="seg-item">警告</div>
            <div class="seg-item">错误</div>
            <div class="seg-item">调试</div>
          </div>
          <div class="log-surface" style="flex:1;">${logStreamRows(4, 8)}</div>
        </div>
        <div class="logs-pad-right">
          ${logInspectorCard(LOGS[4], false)}
        </div>
      </div>`;
    }
  }

  return `<div class="app-screen wide-screen ${isDesktop ? "tier-pointer" : "tier-touch"}">
    ${!isDesktop ? `<div class="pad-status"><span>09:41</span><div class="st-right">${icon("wifi", 13, "#101319", 2.1)}<div class="st-bat"><div class="st-bat-fill"></div></div></div></div>` : ""}
    <div class="wide-shell">
      ${sidebarHtml(activeNav, isDesktop, slide !== 3)}
      <div class="wide-content">
        <div class="wide-toolbar ${isDesktop ? "desktop" : ""}">
          <div class="hdr-title">${title}</div>
          <div class="toolbar-right">
            ${headerRight}
            ${isDesktop ? pcWindowControls() : ""}
          </div>
        </div>
        <div class="wide-body">${body}</div>
      </div>
    </div>
    ${overlay}
  </div>`;
}

// ============================================================================
// Poster Metadata (5 AppGallery Light-Theme Slides x 3 Form Factors)
// ============================================================================

const SLIDES = [
  {
    num: 1,
    slug: "01-overview",
    headline: "一键直连，安全无感",
    tag: {
      phone: "HarmonyOS 原生",
      pad: "平板横屏双栏",
      pc: "2-in-1 桌面原生",
    },
    sub: {
      phone: "状态、实时速率与会话详情一目了然",
      pad: "状态卡片、实时速率与配置列表并行呈现，高效直观",
      pc: "无缝标题栏与双栏工作台，桌面级操作一步直达",
    },
    chips: ["双进程安全隔离", "DTLS 传输加速", "零遥测本机存储"],
    bg: "radial-gradient(120% 85% at 50% 0%, #f1f8e5 0%, #f8fafc 46%, #edf3e8 100%)",
    orb1: "rgba(198, 239, 106, 0.34)",
    orb2: "rgba(19, 138, 90, 0.10)",
  },
  {
    num: 2,
    slug: "02-profiles",
    headline: "多端配置，井然有序",
    tag: {
      phone: "配置与二维码",
      pad: "主从双栏管理",
      pc: "右键菜单与快捷键",
    },
    sub: {
      phone: "分组收藏、极速检索与二维码扫码一键导入",
      pad: "左侧列表筛选、右侧详情面板即时联动，轻松维护多套配置",
      pc: "主从双栏配合原生右键菜单，Ctrl+1~4 快捷键秒级切换",
    },
    chips: ["收藏与即时检索", "二维码导入导出", "本地 0600 权限存储"],
    bg: "radial-gradient(120% 85% at 50% 0%, #eef5fc 0%, #f9fafc 48%, #edf4ef 100%)",
    orb1: "rgba(61, 99, 209, 0.14)",
    orb2: "rgba(198, 239, 106, 0.28)",
  },
  {
    num: 3,
    slug: "03-auth",
    headline: "企业认证，无缝衔接",
    tag: {
      phone: "多维身份认证",
      pad: "居中原生验证弹窗",
      pc: "桌面企业单点登录",
    },
    sub: {
      phone: "全面支持账号密码、客户端证书、多轮 MFA 与网页 SSO",
      pad: "多轮动态口令与服务器认证分组按需拉取，大屏从容应对",
      pc: "原生桌面验证流程，完整兼容多轮 MFA、证书与企业 SSO",
    },
    chips: ["多轮动态 MFA", "网页单点登录", "客户端证书 / TOTP"],
    bg: "radial-gradient(120% 85% at 50% 0%, #edf7f0 0%, #fafbfc 48%, #edf3eb 100%)",
    orb1: "rgba(19, 138, 90, 0.16)",
    orb2: "rgba(198, 239, 106, 0.26)",
  },
  {
    num: 4,
    slug: "04-statistics",
    headline: "吞吐脉络，尽在掌握",
    tag: {
      phone: "实时网络遥测",
      pad: "双栏数据看板",
      pc: "桌面实时监控",
    },
    sub: {
      phone: "实时上传下载速率波形，分配 IP、DNS 与路由清晰呈现",
      pad: "左侧速率波形与累计流量，右侧网络地址与路由同屏展开",
      pc: "宽屏双栏网络面板，实时吞吐曲线与路由详情完整呈现",
    },
    chips: ["实时速率曲线", "IPv4 / IPv6 双栈", "DNS 与分流路由详情"],
    bg: "radial-gradient(120% 85% at 50% 0%, #edf3fd 0%, #f9fbfd 48%, #eef4f7 100%)",
    orb1: "rgba(61, 99, 209, 0.18)",
    orb2: "rgba(19, 138, 90, 0.14)",
  },
  {
    num: 5,
    slug: "05-logs-settings",
    headline: "专业日志，原生美学",
    tag: {
      phone: "分级日志与归档",
      pad: "常驻日志检查器",
      pc: "可收起详情抽屉",
    },
    sub: {
      phone: "关键词检索、五级过滤与按日归档导出，排障得心应手",
      pad: "平板横屏常驻双栏检查器，实时日志与完整原文同步查看",
      pc: "单行内联筛选工具栏配右侧详情抽屉，桌面排障高效清晰",
    },
    chips: ["五级过滤与检索", "按日归档导出", "浅色 / 深色随系统切换"],
    bg: "radial-gradient(120% 85% at 50% 0%, #f2f5f0 0%, #f9fafc 48%, #eef1f6 100%)",
    orb1: "rgba(198, 239, 106, 0.28)",
    orb2: "rgba(61, 99, 209, 0.12)",
  },
];

// ============================================================================
// Poster CSS (Unified Light Paper System + AppGallery Editorial Chrome)
// ============================================================================

const BASE_CSS = `
:root{
  --bg:#f6f6f9; --fg:#101319; --card:#ffffff; --popover:#ffffff;
  --muted:#5f636a; --faint:#9a9da3; --muted-bg:#eeeff2;
  --sidebar:#ecedf0; --sidebar-border:#dedee1; --sidebar-sel:#ffffff;
  --inset:#f1f2f5; --raised:#f3f4f7; --field:#ffffff;
  --line:#e4e4e7; --border:#d0d2d6; --input:#c6c8cc;
  --brand-fill:#c6ef6a; --on-brand:#13160c;
  --brand:#4a840a; --brand-tint:rgba(140,192,29,.16);
  --stop-fill:#d43a4a; --on-stop:#ffffff;
  --up:#138a5a; --up-tint:rgba(19,138,90,.11);
  --warn:#b26a00; --warn-tint:rgba(178,106,0,.12);
  --danger:#d43a4a; --danger-tint:rgba(212,58,74,.10);
  --info:#3d63d1; --info-tint:rgba(61,99,209,.10);
  --sans:-apple-system,BlinkMacSystemFont,"PingFang SC","HarmonyOS Sans SC","SF Pro Display","Hiragino Sans GB",sans-serif;
  --mono:"SF Mono",ui-monospace,Menlo,Consolas,monospace;
}
*{box-sizing:border-box;margin:0;padding:0;-webkit-font-smoothing:antialiased;}
html,body{width:100%;height:100%;overflow:hidden;font-family:var(--sans);color:var(--fg);}

.poster{
  position:relative;width:100%;height:100%;overflow:hidden;
  display:flex;flex-direction:column;align-items:center;
}
.poster-grid{
  position:absolute;inset:0;pointer-events:none;
  background-image:radial-gradient(rgba(16,19,25,.055) 1px, transparent 1px);
  background-size:28px 28px;
  mask-image:linear-gradient(180deg, rgba(0,0,0,.55) 0%, rgba(0,0,0,.18) 55%, rgba(0,0,0,0) 100%);
}
.poster-orb{position:absolute;border-radius:50%;filter:blur(72px);pointer-events:none;}
.poster-watermark{position:absolute;pointer-events:none;opacity:.055;}

.editorial{
  position:relative;z-index:2;text-align:center;
  display:flex;flex-direction:column;align-items:center;
}
.eyebrow-pill{
  display:inline-flex;align-items:center;gap:9px;
  background:rgba(255,255,255,.86);
  border:1px solid rgba(16,19,25,.09);
  box-shadow:0 2px 10px rgba(16,19,25,.04), inset 0 1px 0 #fff;
  border-radius:999px;padding:7px 18px 7px 10px;
  font-weight:600;color:#272d38;letter-spacing:.01em;
}
.eyebrow-dot{width:4px;height:4px;border-radius:50%;background:#4a840a;}
.eyebrow-tag{color:#4a840a;font-weight:600;}
.headline{
  font-weight:700;color:#101319;letter-spacing:-.028em;line-height:1.15;
}
.subtitle{
  font-weight:450;color:#525866;line-height:1.45;
}
.chips{display:flex;align-items:center;justify-content:center;gap:10px;flex-wrap:wrap;}
.chip{
  font-size:16px;font-weight:500;color:#394150;
  background:rgba(255,255,255,.78);border:1px solid rgba(16,19,25,.08);
  padding:6px 14px;border-radius:999px;
  box-shadow:0 1px 4px rgba(16,19,25,.03);
}

/* ==================== Hardware Frames ==================== */
.stage{position:relative;z-index:2;display:flex;align-items:center;justify-content:center;}

.phone-chassis{
  width:726px;height:1428px;border-radius:64px;padding:15px;
  background:linear-gradient(145deg,#e3e6ec 0%,#c7ccd4 32%,#aeb4be 68%,#dcdfe6 100%);
  box-shadow:
    0 46px 100px -24px rgba(16,19,25,.24),
    0 18px 36px -14px rgba(16,19,25,.14),
    inset 0 1px 1px rgba(255,255,255,.95),
    inset 0 -2px 4px rgba(16,19,25,.14);
}
.phone-bezel{
  width:100%;height:100%;border-radius:50px;padding:8px;
  background:#14171d;
  box-shadow:inset 0 0 0 1px rgba(255,255,255,.08);
  overflow:hidden;
}
.phone-viewport{
  width:100%;height:100%;border-radius:42px;overflow:hidden;background:var(--bg);
  position:relative;
}
.phone-scale{
  width:390px;height:790px;
  transform:scale(1.7435897);
  transform-origin:top left;
}

.pad-chassis{
  width:1544px;height:946px;border-radius:38px;padding:15px;
  background:linear-gradient(145deg,#e6e9ef 0%,#ccd1d9 35%,#b8bec8 70%,#dfe3ea 100%);
  box-shadow:
    0 42px 92px -22px rgba(16,19,25,.22),
    0 16px 32px -12px rgba(16,19,25,.12),
    inset 0 1px 1px rgba(255,255,255,.95);
  position:relative;
}
.pad-bezel{
  width:100%;height:100%;border-radius:26px;padding:11px;
  background:#14171d;position:relative;overflow:hidden;
}
.pad-cam{
  position:absolute;top:4px;left:50%;transform:translateX(-50%);
  width:7px;height:7px;border-radius:50%;background:#2a303c;
  box-shadow:inset 0 0 2px #000;
}
.pad-viewport{
  width:100%;height:100%;border-radius:17px;overflow:hidden;background:var(--bg);
  position:relative;
}
.pad-scale{
  width:1180px;height:706px;
  transform:scale(1.2644068);
  transform-origin:top left;
}

.pc-stage-wrap{
  display:flex;flex-direction:column;align-items:center;gap:14px;
}
.pc-window{
  width:1520px;height:786px;border-radius:18px;overflow:hidden;
  background:var(--bg);
  border:1px solid rgba(16,19,25,.14);
  box-shadow:
    0 44px 96px -22px rgba(16,19,25,.22),
    0 14px 30px -10px rgba(16,19,25,.12),
    0 0 0 1px rgba(255,255,255,.65) inset;
  position:relative;
}
.pc-scale{
  width:1280px;height:662px;
  transform:scale(1.1875);
  transform-origin:top left;
}
.pc-dock{
  display:flex;align-items:center;gap:12px;
  padding:8px 18px;border-radius:18px;
  background:rgba(255,255,255,.72);
  border:1px solid rgba(16,19,25,.08);
  box-shadow:0 8px 24px rgba(16,19,25,.08), inset 0 1px 0 #fff;
  backdrop-filter:blur(16px);
}
.dock-dot{
  width:30px;height:30px;border-radius:9px;
  background:linear-gradient(180deg,#f3f5f8 0%,#e6e9ef 100%);
  border:1px solid rgba(16,19,25,.08);
  display:flex;align-items:center;justify-content:center;
}
.dock-active{position:relative;}
.dock-active::after{
  content:"";position:absolute;bottom:-5px;left:50%;transform:translateX(-50%);
  width:10px;height:3px;border-radius:2px;background:#4a840a;
}

/* ==================== App UI Interior ==================== */
.app-screen{
  width:100%;height:100%;background:var(--bg);color:var(--fg);
  display:flex;flex-direction:column;position:relative;overflow:hidden;
}

.phone-status{
  height:40px;padding:0 22px;display:flex;align-items:center;justify-content:space-between;
  flex-shrink:0;background:var(--bg);
}
.st-time{font-size:14px;font-weight:600;font-variant-numeric:tabular-nums;}
.st-pill{width:76px;height:20px;border-radius:999px;background:#101319;}
.st-right{display:flex;align-items:center;gap:7px;}
.st-bat{width:22px;height:11px;border-radius:3px;border:1.4px solid #101319;padding:1.5px;position:relative;}
.st-bat::after{content:"";position:absolute;right:-3px;top:2.5px;width:2px;height:4px;border-radius:0 1px 1px 0;background:#101319;}
.st-bat-fill{width:100%;height:100%;border-radius:1px;background:#101319;}

.pad-status{
  height:26px;padding:0 20px;display:flex;align-items:center;justify-content:space-between;
  font-size:12px;font-weight:600;background:var(--bg);flex-shrink:0;
  font-variant-numeric:tabular-nums;
}

.app-header{
  height:52px;padding:0 16px;display:flex;align-items:center;gap:8px;
  border-bottom:1px solid var(--line);background:var(--bg);flex-shrink:0;
}
.hdr-title{flex:1;font-size:18px;font-weight:600;color:var(--fg);white-space:nowrap;overflow:hidden;text-overflow:ellipsis;}
.hdr-acts{display:flex;align-items:center;gap:6px;}

.app-main{flex:1;overflow:hidden;display:flex;flex-direction:column;}
.p-body{padding:16px;display:flex;flex-direction:column;gap:16px;}

.phone-bottom{
  background:#ffffff;border-top:1px solid var(--line);flex-shrink:0;
  display:flex;flex-direction:column;align-items:center;
}
.phone-tabs{width:100%;height:54px;display:flex;align-items:center;padding:0 8px;}
.phone-tab{
  flex:1;display:flex;flex-direction:column;align-items:center;justify-content:center;gap:3px;
  font-size:11px;font-weight:500;color:var(--muted);
}
.phone-tab.active{color:var(--fg);font-weight:600;}
.phone-home-bar{width:126px;height:4.5px;border-radius:99px;background:rgba(16,19,25,.20);margin:2px 0 8px;}
.phone-home-only{display:flex;justify-content:center;padding:4px 0 2px;background:var(--bg);}

/* Wide Shell (Pad + PC) */
.wide-shell{flex:1;display:flex;min-height:0;overflow:hidden;}
.sidebar{
  width:232px;background:var(--sidebar);border-right:1px solid var(--sidebar-border);
  display:flex;flex-direction:column;flex-shrink:0;
}
.sb-top{height:48px;padding:0 8px;display:flex;align-items:center;}
.sb-brand{
  width:100%;height:36px;padding:0 8px;border-radius:7px;
  display:flex;align-items:center;gap:9px;
}
.sb-brand-name{flex:1;font-size:15px;font-weight:600;letter-spacing:-.015em;color:var(--fg);}
.sb-nav{flex:1;padding:6px 8px 10px;display:flex;flex-direction:column;gap:6px;}
.sb-group{display:flex;flex-direction:column;gap:3px;}
.sb-spacer{flex:1;}
.sb-sep{height:1px;background:var(--sidebar-border);margin:4px 4px;}
.sb-item{
  display:flex;align-items:center;gap:9px;padding:0 10px;
  height:38px;border-radius:9px;border:1px solid transparent;
  font-size:14px;font-weight:500;color:var(--muted);
}
.sidebar.desktop .sb-item{height:30px;border-radius:6px;padding:0 8px;font-size:13px;font-weight:450;}
.sb-item.active{
  background:var(--sidebar-sel);border-color:var(--border);
  color:var(--fg);font-weight:600;
  box-shadow:0 1px 2px rgba(16,19,25,.04);
}
.sidebar.desktop .sb-item.active{font-weight:500;}
.sb-label{flex:1;}
.sb-key{font-size:11px;color:var(--faint);font-variant-numeric:tabular-nums;}
.sb-foot{
  padding:10px 14px;border-top:1px solid var(--sidebar-border);
  display:flex;align-items:center;justify-content:space-between;
  font-size:12px;color:var(--muted);font-variant-numeric:tabular-nums;
}
.sb-status{display:flex;align-items:center;gap:6px;}
.dot-up{width:7px;height:7px;border-radius:50%;background:var(--up);display:inline-block;flex-shrink:0;}
.dot-idle{width:7px;height:7px;border-radius:50%;background:var(--info);display:inline-block;flex-shrink:0;}

.wide-content{flex:1;min-width:0;display:flex;flex-direction:column;background:var(--bg);}
.wide-toolbar{
  height:48px;padding:0 20px;border-bottom:1px solid var(--line);
  display:flex;align-items:center;justify-content:space-between;gap:12px;
  background:var(--bg);flex-shrink:0;
}
.wide-toolbar.desktop .hdr-title{font-size:16px;}
.toolbar-right{display:flex;align-items:center;gap:12px;}
.win-controls{display:flex;align-items:center;gap:6px;margin-left:6px;padding-left:10px;border-left:1px solid var(--line);}
.win-btn{width:26px;height:26px;border-radius:6px;display:flex;align-items:center;justify-content:center;background:rgba(16,19,25,.035);}

.wide-body{flex:1;padding:20px;overflow:hidden;display:flex;flex-direction:column;}
.w-two-col{display:flex;gap:18px;align-items:flex-start;max-width:960px;width:100%;margin:0 auto;}
.w-col-left{width:348px;flex-shrink:0;display:flex;flex-direction:column;gap:16px;}
.w-col-right{flex:1;min-width:0;display:flex;flex-direction:column;gap:16px;}
.w-master-detail{display:flex;gap:18px;align-items:flex-start;max-width:1060px;width:100%;margin:0 auto;}
.w-list-col{width:386px;flex-shrink:0;}
.w-detail-col{flex:1;min-width:0;}
.w-equal-col{display:flex;gap:18px;align-items:flex-start;max-width:1060px;width:100%;margin:0 auto;}
.w-eq-col{flex:1;min-width:0;display:flex;flex-direction:column;gap:16px;}

/* Cards & Surfaces */
.card{
  background:var(--card);border:1px solid var(--border);border-radius:16px;
  padding:16px;display:flex;flex-direction:column;gap:12px;
}
.tier-pointer .card{border-radius:10px;padding:14px;gap:10px;}
.card-head{display:flex;align-items:center;justify-content:space-between;gap:10px;}
.card-title{font-size:15px;font-weight:600;color:var(--fg);}
.tier-pointer .card-title{font-size:13.5px;}
.meta-text{font-size:12px;color:var(--muted);font-variant-numeric:tabular-nums;}
.link-act{font-size:13px;font-weight:500;color:var(--brand);}
.sep{height:1px;background:var(--line);width:100%;}

.sec{display:flex;flex-direction:column;gap:8px;}
.sec-head{display:flex;align-items:center;justify-content:space-between;padding:0 2px;}
.sec-title{font-size:15px;font-weight:600;color:var(--fg);}
.tier-pointer .sec-title{font-size:13px;}

/* Hero Card */
.hero-top{display:flex;align-items:center;gap:12px;}
.hero-icon{
  width:46px;height:46px;border-radius:14px;border:1px solid var(--border);
  background:var(--raised);display:flex;align-items:center;justify-content:center;flex-shrink:0;
}
.hero-icon.connected{
  background:var(--brand-fill);border-color:#9cc43a;
  box-shadow:inset 0 1px 0 rgba(255,255,255,.55);
}
.hero-meta{flex:1;min-width:0;display:flex;flex-direction:column;gap:3px;}
.hero-title-row{display:flex;align-items:center;gap:8px;flex-wrap:wrap;}
.hero-title{font-size:17px;font-weight:600;color:var(--fg);}
.hero-sub{font-size:13px;color:var(--muted);font-variant-numeric:tabular-nums;}
.tier-pointer .hero-sub{font-size:12px;}

/* Buttons */
.btn{
  display:inline-flex;align-items:center;justify-content:center;gap:6px;
  height:40px;padding:0 14px;border-radius:10px;font-size:14px;font-weight:600;
  border:1px solid transparent;white-space:nowrap;
}
.tier-pointer .btn{height:28px;border-radius:7px;font-size:13px;padding:0 11px;}
.btn.sm{height:34px;padding:0 12px;font-size:13px;border-radius:9px;}
.tier-pointer .btn.sm{height:25px;padding:0 9px;font-size:12px;border-radius:6px;}
.btn.full{width:100%;height:44px;font-size:15px;}
.tier-pointer .btn.full{height:34px;font-size:13.5px;}
.btn.primary{
  background:var(--brand-fill);color:var(--on-brand);border-color:#9cc43a;
  box-shadow:inset 0 1px 0 rgba(255,255,255,.55), 0 1px 0 rgba(60,90,0,.14);
}
.btn.stop{
  background:var(--stop-fill);color:var(--on-stop);border-color:#b32837;
  box-shadow:inset 0 1px 0 rgba(255,255,255,.28), 0 1px 0 rgba(90,10,20,.16);
}
.btn.secondary{
  background:var(--raised);color:var(--fg);border-color:var(--border);
  box-shadow:inset 0 1px 0 #fff, 0 1px 0 rgba(10,15,25,.04);
}
.btn.danger-outline{
  background:#fff;color:var(--danger);border-color:rgba(212,58,74,.35);
}
.actions-row{display:flex;align-items:center;gap:8px;flex-wrap:wrap;}
.icon-btn-sm{
  width:30px;height:30px;border-radius:8px;display:flex;align-items:center;justify-content:center;
  background:transparent;
}

.spin{
  width:15px;height:15px;border-radius:50%;
  border:2px solid rgba(19,22,12,.22);border-top-color:#13160c;display:inline-block;
}

/* Badges & Toggle */
.badge{
  display:inline-flex;align-items:center;padding:2px 8px;border-radius:999px;
  font-size:11.5px;font-weight:600;line-height:16px;white-space:nowrap;
}
.toggle-row{display:flex;align-items:center;justify-content:space-between;gap:12px;}
.toggle-label{font-size:14px;font-weight:500;color:var(--fg);}
.toggle-desc{font-size:12px;color:var(--muted);margin-top:2px;}
.toggle{
  width:44px;height:26px;border-radius:999px;background:#dfe1e6;padding:3px;
  display:flex;align-items:center;flex-shrink:0;
}
.toggle.on{background:#a5db3b;justify-content:flex-end;}
.toggle-knob{width:20px;height:20px;border-radius:50%;background:#fff;box-shadow:0 1px 3px rgba(0,0,0,.18);}

/* Speed Grid */
.speed-grid{
  background:var(--card);border:1px solid var(--border);border-radius:16px;
  display:flex;align-items:stretch;overflow:hidden;
}
.tier-pointer .speed-grid{border-radius:10px;}
.speed-cell{flex:1;padding:13px 16px;display:flex;flex-direction:column;gap:5px;}
.speed-grid.compact .speed-cell{padding:10px 14px;}
.speed-sep{width:1px;background:var(--line);}
.speed-head{display:flex;align-items:center;gap:5px;font-size:12.5px;color:var(--muted);font-weight:500;}
.speed-val{font-size:20px;font-weight:650;color:var(--fg);font-variant-numeric:tabular-nums;letter-spacing:-.02em;}
.speed-grid.compact .speed-val{font-size:17px;}

/* Details */
.detail-row{
  display:flex;align-items:center;justify-content:space-between;gap:12px;
  padding:8px 0;border-bottom:1px solid var(--line);font-size:13.5px;
}
.tier-pointer .detail-row{padding:6px 0;font-size:12.5px;}
.detail-row.last{border-bottom:none;padding-bottom:2px;}
.detail-k{color:var(--muted);flex-shrink:0;}
.detail-v{color:var(--fg);font-weight:500;font-variant-numeric:tabular-nums;text-align:right;overflow:hidden;text-overflow:ellipsis;white-space:nowrap;}

/* Lists */
.list-surface{
  background:var(--card);border:1px solid var(--border);border-radius:16px;overflow:hidden;
}
.tier-pointer .list-surface{border-radius:10px;}
.list-row{
  display:flex;align-items:center;gap:12px;padding:12px 14px;
  border-bottom:1px solid var(--line);background:var(--card);
}
.tier-pointer .list-row{padding:9px 12px;gap:10px;}
.list-row.last{border-bottom:none;}
.list-row.selected{background:var(--brand-tint);}
.icon-tile{
  width:34px;height:34px;border-radius:10px;background:var(--raised);border:1px solid var(--line);
  display:flex;align-items:center;justify-content:center;flex-shrink:0;
}
.tier-pointer .icon-tile{width:30px;height:30px;border-radius:8px;}
.row-body{flex:1;min-width:0;display:flex;flex-direction:column;gap:2px;}
.row-title-line{display:flex;align-items:center;gap:7px;}
.row-title{font-size:14.5px;font-weight:600;color:var(--fg);white-space:nowrap;overflow:hidden;text-overflow:ellipsis;}
.tier-pointer .row-title{font-size:13.5px;}
.row-sub{font-size:12.5px;color:var(--muted);white-space:nowrap;overflow:hidden;text-overflow:ellipsis;font-variant-numeric:tabular-nums;}
.row-actions{display:flex;align-items:center;gap:10px;flex-shrink:0;}

/* Segmented Tabs & Search */
.filter-bar{display:flex;align-items:center;gap:8px;}
.seg{
  display:flex;background:var(--inset);border:1px solid var(--line);border-radius:10px;padding:3px;gap:2px;
}
.seg.inline{display:inline-flex;flex-shrink:0;}
.tier-pointer .seg{border-radius:8px;padding:2px;}
.seg-item{
  flex:1;text-align:center;padding:5px 10px;border-radius:7px;font-size:13px;font-weight:500;color:var(--muted);
  white-space:nowrap;
}
.tier-pointer .seg-item{padding:4px 9px;font-size:12px;border-radius:6px;}
.sm-seg .seg-item{padding:4px 8px;font-size:12px;}
.seg-item.active{background:#fff;color:var(--fg);font-weight:600;box-shadow:0 1px 2px rgba(16,19,25,.08);}
.seg-cnt{color:var(--faint);font-weight:500;margin-left:2px;font-variant-numeric:tabular-nums;}
.search-field{
  height:38px;padding:0 12px;border-radius:10px;background:var(--field);
  border:1px solid var(--input);display:flex;align-items:center;gap:8px;
  font-size:13.5px;color:var(--faint);min-width:0;flex-shrink:0;
}
.filter-bar .search-field,.logs-pc-bar .search-field{flex:1;}
.tier-pointer .search-field{height:30px;border-radius:7px;font-size:12.5px;padding:0 10px;}

/* QR Promo Accent on Phone Profiles */
.qr-promo-card{
  background:linear-gradient(135deg,#ffffff 0%,#f4faea 100%);
  border:1px solid #cfe0b0;border-radius:16px;padding:14px 16px;
  display:flex;align-items:center;gap:14px;
}
.qr-mini-box{
  width:56px;height:56px;border-radius:12px;background:#ffffff;border:1px solid var(--border);
  display:flex;align-items:center;justify-content:center;flex-shrink:0;
}
.qr-promo-text{display:flex;flex-direction:column;gap:3px;}

/* PC Context Menu */
.ctx-menu{
  position:absolute;right:14px;top:148px;width:186px;z-index:5;
  background:#ffffff;border:1px solid var(--border);border-radius:10px;padding:4px;
  box-shadow:0 14px 32px -8px rgba(16,19,25,.22), 0 2px 6px rgba(16,19,25,.06);
  display:flex;flex-direction:column;gap:1px;
}
.ctx-item{
  height:28px;padding:0 9px;border-radius:6px;display:flex;align-items:center;gap:8px;
  font-size:12.5px;color:var(--fg);
}
.ctx-item.active{background:var(--inset);}
.ctx-item.danger{color:var(--danger);}
.ctx-sep{height:1px;background:var(--line);margin:3px 4px;}

/* Auth Modal / Sheet */
.modal-scrim{
  position:absolute;inset:0;background:rgba(16,19,25,.38);z-index:10;
  display:flex;justify-content:center;
}
.modal-scrim.sheet-align{align-items:flex-end;}
.modal-scrim.dialog-align{align-items:center;padding:24px;}
.sheet-card{
  width:100%;background:#ffffff;border-radius:22px 22px 0 0;
  border-top:1px solid var(--border);
  box-shadow:0 -12px 36px rgba(16,19,25,.18);
  display:flex;flex-direction:column;padding-bottom:14px;
}
.sheet-grabber{width:38px;height:4.5px;border-radius:99px;background:#d4d6dc;margin:9px auto 4px;}
.dialog-card{
  width:460px;background:#ffffff;border-radius:16px;border:1px solid var(--border);
  box-shadow:0 24px 60px -12px rgba(16,19,25,.28);
  display:flex;flex-direction:column;overflow:hidden;
}
.modal-head{
  padding:14px 16px 12px;border-bottom:1px solid var(--line);
  display:flex;align-items:flex-start;justify-content:space-between;gap:10px;
}
.modal-title{font-size:17px;font-weight:650;color:var(--fg);}
.modal-sub{font-size:12.5px;color:var(--muted);margin-top:2px;}
.modal-body{padding:16px;display:flex;flex-direction:column;gap:13px;}
.modal-foot{padding:12px 16px 14px;border-top:1px solid var(--line);display:flex;gap:10px;}
.banner-box{
  display:flex;align-items:flex-start;gap:9px;padding:10px 12px;border-radius:10px;
  background:var(--brand-tint);border:1px solid rgba(74,132,10,.20);
  font-size:12.5px;line-height:1.45;color:#283a14;
}
.field-group{display:flex;flex-direction:column;gap:5px;}
.field-label{font-size:12.5px;font-weight:600;color:var(--fg);}
.select-box,.input-box{
  height:40px;padding:0 12px;border-radius:10px;background:#fff;border:1px solid var(--input);
  display:flex;align-items:center;justify-content:space-between;font-size:14px;color:var(--fg);
}
.input-box.focused{
  border-color:var(--brand);box-shadow:0 0 0 3px rgba(74,132,10,.15);
  justify-content:flex-start;gap:2px;font-family:var(--mono);font-size:15px;font-weight:600;letter-spacing:.08em;
}
.caret{width:2px;height:18px;background:var(--brand);border-radius:1px;}
.auth-methods-hint{display:flex;gap:6px;flex-wrap:wrap;padding-top:2px;}
.pill-chip{
  display:inline-flex;align-items:center;gap:5px;font-size:11.5px;font-weight:500;
  color:var(--muted);background:var(--inset);padding:4px 9px;border-radius:999px;
}

/* Traffic Chart */
.chart-box{display:flex;flex-direction:column;gap:10px;}
.chart-plot{
  display:flex;align-items:flex-end;gap:4px;padding:8px 4px 0;
  border-bottom:1px solid var(--line);
  background:repeating-linear-gradient(180deg, transparent 0, transparent 31px, rgba(16,19,25,.035) 31px, rgba(16,19,25,.035) 32px);
}
.chart-col{flex:1;height:100%;display:flex;align-items:flex-end;gap:1.5px;}
.chart-bar{flex:1;border-radius:2.5px 2.5px 0 0;}
.chart-bar.down{background:linear-gradient(180deg,#4c73e0 0%,#3d63d1 100%);}
.chart-bar.up{background:linear-gradient(180deg,#20a870 0%,#138a5a 100%);}
.chart-legend{display:flex;justify-content:space-between;font-size:12px;color:var(--muted);font-variant-numeric:tabular-nums;}
.legend-item{display:flex;align-items:center;gap:6px;}
.legend-dot{width:8px;height:8px;border-radius:2px;display:inline-block;}

/* Logs */
.status-note{
  display:flex;align-items:center;gap:6px;font-size:12px;color:var(--muted);
  font-variant-numeric:tabular-nums;padding:0 2px;
}
.faint{color:var(--faint);}
.log-surface{
  background:var(--card);border:1px solid var(--border);border-radius:14px;overflow:hidden;
  display:flex;flex-direction:column;
}
.tier-pointer .log-surface{border-radius:10px;}
.log-row{
  display:flex;align-items:flex-start;gap:9px;padding:10px 13px;
  border-bottom:1px solid var(--line);font-family:var(--mono);font-size:12px;line-height:1.45;
}
.tier-pointer .log-row{padding:8px 12px;font-size:11.8px;}
.log-row.selected{background:var(--brand-tint);}
.log-ts{color:var(--faint);flex-shrink:0;font-variant-numeric:tabular-nums;}
.log-msg{color:#2a2d33;flex:1;min-width:0;word-break:break-word;}

.logs-pad-split{display:flex;gap:16px;flex:1;min-height:0;}
.logs-pad-left{flex:1.15;min-width:0;display:flex;flex-direction:column;gap:10px;}
.logs-pad-right{flex:0.85;min-width:300px;display:flex;flex-direction:column;}

.logs-pc-wrap{display:flex;flex-direction:column;gap:10px;flex:1;min-height:0;}
.logs-pc-bar{display:flex;align-items:center;gap:10px;}
.logs-pc-split{display:flex;gap:14px;flex:1;min-height:0;}
.inspector-drawer{
  width:330px;flex-shrink:0;background:var(--card);border:1px solid var(--border);
  border-radius:10px;padding:14px;display:flex;flex-direction:column;
}
.inspector-pane{display:flex;flex-direction:column;gap:12px;}
.code-well{
  background:var(--inset);border:1px solid var(--line);border-radius:10px;
  padding:11px 12px;font-family:var(--mono);font-size:12px;line-height:1.5;color:#22262e;
}
`;

// ============================================================================
// Poster Page Generators for Phone / Pad / PC
// ============================================================================

function buildPosterHtml(form, slide) {
  const isPhone = form === "phone";
  const isPad = form === "pad";
  const isPc = form === "pc";

  const watermarkSvg = `<svg width="520" height="520" viewBox="0 0 24 24" fill="none" stroke="#4a840a" stroke-width="0.9" stroke-linecap="round">
    <path d="M 18.39 6.47 A 8.6 8.6 0 1 0 18.39 17.53"/>
    <path d="M 15.71 8.72 A 5.1 5.1 0 1 0 15.71 15.28"/>
    <circle cx="11.8" cy="12" r="1.65"/>
    <path d="M 13.45 12 H 20.6"/>
  </svg>`;

  let formCss = "";
  let stageHtml = "";

  if (isPhone) {
    formCss = `
      .poster{padding:76px 56px 52px;justify-content:space-between;}
      .editorial{gap:16px;margin-bottom:14px;}
      .eyebrow-pill{font-size:21px;padding:8px 20px 8px 10px;}
      .headline{font-size:74px;}
      .subtitle{font-size:29px;max-width:880px;}
      .chip{font-size:20px;padding:8px 18px;}
      .poster-watermark{top:-120px;right:-150px;}
    `;
    stageHtml = `<div class="stage">
      <div class="phone-chassis">
        <div class="phone-bezel">
          <div class="phone-viewport">
            <div class="phone-scale">${renderPhoneScreen(slide.num)}</div>
          </div>
        </div>
      </div>
    </div>`;
  } else if (isPad) {
    formCss = `
      .poster{padding:48px 72px 42px;justify-content:space-between;}
      .editorial{gap:12px;}
      .eyebrow-pill{font-size:18px;padding:6px 18px 6px 9px;}
      .headline{font-size:58px;}
      .subtitle{font-size:24px;}
      .chips{gap:10px;margin-top:2px;}
      .chip{font-size:16px;padding:5px 14px;}
      .poster-watermark{top:-140px;right:-110px;}
    `;
    stageHtml = `<div class="stage">
      <div class="pad-chassis">
        <div class="pad-bezel">
          <div class="pad-cam"></div>
          <div class="pad-viewport">
            <div class="pad-scale">${renderWideScreen(slide.num, false)}</div>
          </div>
        </div>
      </div>
    </div>`;
  } else {
    formCss = `
      .poster{padding:38px 72px 26px;justify-content:space-between;}
      .editorial{gap:10px;}
      .eyebrow-pill{font-size:16px;padding:5px 16px 5px 8px;}
      .headline{font-size:50px;}
      .subtitle{font-size:21px;}
      .poster-watermark{top:-160px;right:-90px;}
    `;
    stageHtml = `<div class="stage">
      <div class="pc-stage-wrap">
        <div class="pc-window">
          <div class="pc-scale">${renderWideScreen(slide.num, true)}</div>
        </div>
        <div class="pc-dock">
          <div class="dock-dot">${icon("globe", 15, "#6b7280", 1.8)}</div>
          <div class="dock-dot">${icon("file-text", 15, "#6b7280", 1.8)}</div>
          <div class="dock-active">${appIconTile(32, 9, 20)}</div>
          <div class="dock-dot">${icon("server", 15, "#6b7280", 1.8)}</div>
          <div class="dock-dot">${icon("settings", 15, "#6b7280", 1.8)}</div>
        </div>
      </div>
    </div>`;
  }

  return `<!doctype html>
<html lang="zh-CN">
<head>
<meta charset="utf-8">
<style>${BASE_CSS}${formCss}</style>
</head>
<body>
  <div class="poster" style="background:${slide.bg};">
    <div class="poster-grid"></div>
    <div class="poster-orb" style="width:680px;height:680px;top:-180px;left:-140px;background:${slide.orb1};"></div>
    <div class="poster-orb" style="width:620px;height:620px;bottom:-160px;right:-120px;background:${slide.orb2};"></div>
    <div class="poster-watermark">${watermarkSvg}</div>

    <div class="editorial">
      <div class="eyebrow-pill">
        ${appIconTile(isPhone ? 30 : 26, 8, isPhone ? 19 : 16)}
        <span>H-OpenConnect</span>
        <span class="eyebrow-dot"></span>
        <span class="eyebrow-tag">${slide.tag[form]}</span>
      </div>
      <h1 class="headline">${slide.headline}</h1>
      <p class="subtitle">${slide.sub[form]}</p>
      ${
        isPhone
          ? `<div class="chips">${slide.chips.map((c) => `<span class="chip">${c}</span>`).join("")}</div>`
          : ""
      }
    </div>

    ${stageHtml}
  </div>
</body>
</html>`;
}

const TARGETS = [
  { form: "phone", width: 1080, height: 1920 },
  { form: "pad", width: 1920, height: 1280 },
  { form: "pc", width: 1920, height: 1080 },
];

const generated = [];
for (const target of TARGETS) {
  for (const slide of SLIDES) {
    const html = buildPosterHtml(target.form, slide);
    // Strict zero-VPN compliance guard
    if (/\bvpn\b/i.test(html)) {
      throw new Error(`Forbidden VPN string detected in ${target.form}/${slide.slug}`);
    }
    const htmlPath = path.join(TMP_HTML, `${target.form}-${slide.slug}.html`);
    const pngPath = path.join(OUT_ROOT, target.form, `${slide.slug}.png`);
    fs.writeFileSync(htmlPath, html, "utf8");
    execFileSync(
      CHROME,
      [
        "--headless=new",
        "--disable-gpu",
        "--hide-scrollbars",
        "--force-device-scale-factor=1",
        `--window-size=${target.width},${target.height}`,
        `--screenshot=${pngPath}`,
        `file://${htmlPath}`,
      ],
      { stdio: "pipe" },
    );
    generated.push({ pngPath, expected: `${target.width}x${target.height}` });
    console.log(`Rendered ${path.relative(REPO_ROOT, pngPath)} (${target.width}x${target.height})`);
  }
}

console.log(`Done: ${generated.length} posters generated.`);

fs.rmSync(TMP_HTML, { recursive: true, force: true });
