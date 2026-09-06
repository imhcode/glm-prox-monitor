import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";

// ---------- Types (mirror of Rust structs) ----------

interface CurrentUsage {
  tokens_used: number;
  window_started_at: string;
  window_ends_at: string;
  remaining_tokens: number;
}

interface Stats {
  key: string;
  name: string;
  model: string;
  token_limit: number;
  expiry_date: string;
  created_at: string;
  last_used: string;
  is_expired: boolean;
  current_usage: CurrentUsage | null;
  total_requests: number;
  total_lifetime_tokens: number;
}

interface Snapshot {
  kind: "ok" | "limited" | "failed";
  fetched_at: number | null;
  stats: Stats | null;
  message: string | null;
  window_ends_at: string | null;
}

interface Config {
  base_url: string;
  token: string;
  interval_secs: number;
  bar_visible: boolean;
  bar_x?: number | null;
  bar_y?: number | null;
  compact: boolean;
  theme: string;
}

// ---------- Helpers ----------

/** Padanan Convert-ToWIB: ISO -> "yyyy-MM-dd HH:mm:ss WIB" */
function toWIB(iso: string | null | undefined): string {
  if (!iso) return "—";
  const d = new Date(iso);
  if (isNaN(d.getTime())) return iso;
  const parts = new Intl.DateTimeFormat("en-CA", {
    timeZone: "Asia/Jakarta",
    year: "numeric",
    month: "2-digit",
    day: "2-digit",
    hour: "2-digit",
    minute: "2-digit",
    second: "2-digit",
    hourCycle: "h23",
  }).formatToParts(d);
  const g = (t: string) => parts.find((p) => p.type === t)?.value ?? "00";
  return `${g("year")}-${g("month")}-${g("day")} ${g("hour")}:${g("minute")}:${g("second")} WIB`;
}

function fmtTok(n: number): string {
  const v = Number(n ?? 0);
  if (!isFinite(v)) return "0";
  if (v >= 1e9) return (v / 1e9).toFixed(2) + "B";
  if (v >= 1e6) return (v / 1e6).toFixed(1) + "M";
  if (v >= 1e3) return (v / 1e3).toFixed(0) + "K";
  return String(v);
}

function fmtInt(n: number): string {
  const v = Number(n ?? 0);
  if (!isFinite(v)) return "0";
  return v.toLocaleString("id-ID");
}

/** ISO -> milidetik tersisa (bisa negatif) */
function msUntil(iso: string | null | undefined): number | null {
  if (!iso) return null;
  const d = new Date(iso).getTime();
  if (isNaN(d)) return null;
  return d - Date.now();
}

/** countdown pendek: "2j 14m" / "14m 05s" */
function fmtCountdown(ms: number): string {
  if (ms <= 0) return "sekarang";
  const s = Math.floor(ms / 1000);
  const h = Math.floor(s / 3600);
  const m = Math.floor((s % 3600) / 60);
  const sec = s % 60;
  if (h > 0) return `${h}j ${String(m).padStart(2, "0")}m`;
  if (m > 0) return `${m}m ${String(sec).padStart(2, "0")}s`;
  return `${sec}s`;
}

// ---------- State ----------

let snapshot: Snapshot | null = null;
let config: Config | null = null;

const $ = <T extends HTMLElement>(id: string) => document.getElementById(id) as T;

const pill = $("pill");
const pillFill = $("pill-fill");
const pillPct = $("pill-pct");
const pillTok = $("pill-tok");
const pillReset = $("pill-reset");
const pillChip = $("pill-chip");
const panel = $("panel");
const panelRows = $("panel-rows");
const panelStatus = $("panel-status");

// ---------- Rendering ----------

function usageInfo(stats: Stats): { usedPct: number; remainingPct: number } | null {
  if (!stats.current_usage) return null;
  const limit = Number(stats.token_limit ?? 0);
  const used = Number(stats.current_usage.tokens_used ?? 0);
  if (!(limit > 0) || !isFinite(limit) || !isFinite(used)) return null;
  const usedPct = (used / limit) * 100;
  return { usedPct, remainingPct: 100 - usedPct };
}

// ---------- Theme & compact ----------

const THEMES = ["dark", "light", "midnight", "oled"];

function applyBodyClasses() {
  const theme = config && THEMES.includes(config.theme) ? config.theme : "dark";
  THEMES.forEach((t) => document.body.classList.toggle(`theme-${t}`, t === theme));
  document.body.classList.toggle("compact", !!config?.compact);
}

function renderPill() {
  try {
    renderPillInner();
  } catch (e) {
    // render gagal tidak boleh meninggalkan pill setengah-updated
    pill.classList.remove("warn", "crit", "limited");
    pill.classList.add("offline");
    pillFill.style.width = "0%";
    pillPct.textContent = "error";
    pillTok.textContent = "";
    pillReset.textContent = "";
    console.error("renderPill failed", e);
  }
}

function renderPillInner() {
  pill.classList.remove("warn", "crit", "limited", "offline");

  if (!snapshot) {
    pillPct.textContent = "memuat…";
    pillTok.textContent = "";
    pillReset.textContent = "";
    pillFill.style.width = "0%";
    pill.classList.add("offline");
    return;
  }

  if (snapshot.kind === "ok" && snapshot.stats) {
    const stats = snapshot.stats;
    pillChip.textContent = "GLM";
    const info = usageInfo(stats);
    if (info) {
      pillFill.style.width = `${Math.min(100, Math.max(0, info.usedPct))}%`;
      const suffix = document.body.classList.contains("compact") ? "" : " sisa";
      pillPct.textContent = `${info.remainingPct.toFixed(0)}%${suffix}`;
      if (info.remainingPct < 5) pill.classList.add("crit");
      else if (info.remainingPct < 20) pill.classList.add("warn");
      pillTok.textContent = `${fmtTok(stats.current_usage!.remaining_tokens)} tok`;
    } else {
      pillFill.style.width = "0%";
      pillPct.textContent = "—";
      pillTok.textContent = "";
    }
    renderResetSlot();
    return;
  }

  if (snapshot.kind === "limited") {
    pillFill.style.width = "100%";
    pillPct.textContent = "0% sisa";
    pillTok.textContent = "limit";
    pill.classList.add("limited");
    renderResetSlot();
    return;
  }

  // failed / offline
  pillFill.style.width = "0%";
  pillPct.textContent = "offline";
  pillTok.textContent = "";
  pillReset.textContent = "";
  pill.classList.add("offline");
}

function renderResetSlot() {
  const wea =
    snapshot?.kind === "ok" && snapshot.stats?.current_usage
      ? snapshot.stats.current_usage.window_ends_at
      : snapshot?.kind === "limited"
        ? snapshot.window_ends_at
        : null;
  const ms = msUntil(wea);
  if (ms === null) {
    pillReset.textContent = "";
    return;
  }
  pillReset.textContent = ms <= 0 ? "reset…" : `reset ${fmtCountdown(ms)}`;
}

function row(label: string, value: string, cls = ""): string {
  return `<span class="row-label">${label}</span><span class="row-value ${cls}">${value}</span>`;
}

function renderPanel() {
  try {
    renderPanelInner();
  } catch (e) {
    panelRows.innerHTML =
      row("Status", "Gagal merender data", "crit") + row("Error", escapeHtml(String(e)));
    console.error("renderPanel failed", e);
  }
}

function renderPanelInner() {
  if (!snapshot) {
    panelRows.innerHTML = row("Status", "memuat…", "dim");
    return;
  }

  if (snapshot.kind === "ok" && snapshot.stats) {
    const s = snapshot.stats;
    const info = usageInfo(s);
    const pctCls = info ? (info.remainingPct < 5 ? "crit" : info.remainingPct < 20 ? "warn" : "ok") : "";
    const rows: string[] = [
      row("Nama", escapeHtml(s.name || "—")),
      row("Model", escapeHtml(s.model || "—")),
      row("Status langganan", s.is_expired ? "expired" : "aktif", s.is_expired ? "crit" : "ok"),
    ];
    if (info && s.current_usage) {
      rows.push(
        row("Pakai window", `${info.usedPct.toFixed(1)}% (${fmtInt(s.current_usage.tokens_used)} / ${fmtInt(s.token_limit)})`, pctCls),
        row("Sisa token", `${fmtInt(s.current_usage.remaining_tokens)}`, pctCls),
        row("Window mulai", toWIB(s.current_usage.window_started_at)),
        row("Window berakhir", toWIB(s.current_usage.window_ends_at)),
        row("Reset dalam", fmtCountdown(msUntil(s.current_usage.window_ends_at) ?? 0), pctCls),
      );
    }
    rows.push(
      row("Total request", fmtInt(s.total_requests)),
      row("Total token", fmtInt(s.total_lifetime_tokens) + ` (${fmtTok(s.total_lifetime_tokens)})`),
      row("Terakhir dipakai", toWIB(s.last_used)),
      row("Langganan berakhir", toWIB(s.expiry_date)),
      row("Dibuat", toWIB(s.created_at)),
      row("Key", escapeHtml(s.key || "—")),
    );
    panelRows.innerHTML = rows.join("");
    panelStatus.textContent = snapshot.fetched_at
      ? `Diperbarui ${toWIB(new Date(snapshot.fetched_at).toISOString())} · interval ${config?.interval_secs ?? "?"}s`
      : "";
    return;
  }

  if (snapshot.kind === "limited") {
    panelRows.innerHTML = [
      row("Status", "Kena limit window 5 jam", "crit"),
      row("Pesan", escapeHtml(snapshot.message ?? "—")),
      row("Reset", toWIB(snapshot.window_ends_at)),
      row("Hitung mundur", fmtCountdown(msUntil(snapshot.window_ends_at) ?? 0), "warn"),
    ].join("");
    panelStatus.textContent = "Data akan otomatis diperbarui setelah window reset.";
    return;
  }

  panelRows.innerHTML = row("Status", "Gagal mengambil data", "crit") + row("Error", escapeHtml(snapshot.message ?? "—"));
  panelStatus.textContent = "Akan mencoba lagi otomatis. Cek koneksi atau token di Settings.";
}

function escapeHtml(s: string): string {
  return s.replace(/[&<>"']/g, (c) => ({ "&": "&amp;", "<": "&lt;", ">": "&gt;", '"': "&quot;", "'": "&#39;" }[c]!));
}

function renderAll() {
  renderPill();
  renderPanel();
}

// ---------- Window expand/collapse & drag ----------

let expanded = false;
let pillDownAt: { x: number; y: number } | null = null;
let pillDragging = false;

async function setExpanded(v: boolean) {
  expanded = v;
  panel.classList.toggle("hidden", !v);
  try {
    await invoke("set_expanded", { expanded: v });
  } catch (e) {
    console.error("set_expanded failed", e);
  }
}

pill.addEventListener("mousedown", (e) => {
  pillDownAt = { x: e.clientX, y: e.clientY };
  pillDragging = false;
});

pill.addEventListener("mousemove", (e) => {
  if (!pillDownAt || pillDragging) return;
  if (
    Math.abs(e.clientX - pillDownAt.x) > 6 ||
    Math.abs(e.clientY - pillDownAt.y) > 6
  ) {
    pillDragging = true;
    invoke("start_bar_drag").catch((err) => console.error("start_bar_drag failed", err));
  }
});

pill.addEventListener("click", (e) => {
  // Gerakan > 6px berarti drag posisi, bukan klik — jangan expand.
  if (
    pillDragging ||
    (pillDownAt &&
      (Math.abs(e.clientX - pillDownAt.x) > 6 || Math.abs(e.clientY - pillDownAt.y) > 6))
  ) {
    return;
  }
  setExpanded(!expanded);
});

document.addEventListener("mouseup", () => {
  pillDownAt = null;
  pillDragging = false;
});
$("btn-collapse").addEventListener("click", () => setExpanded(false));
document.addEventListener("keydown", (ev) => {
  if (ev.key === "Escape" && expanded) setExpanded(false);
});

$("btn-refresh").addEventListener("click", async () => {
  panelStatus.textContent = "Menyegarkan…";
  try {
    snapshot = await invoke<Snapshot>("refresh_now");
    renderAll();
  } catch (e) {
    panelStatus.textContent = `Gagal: ${e}`;
  }
});

$("btn-hide").addEventListener("click", async () => {
  try {
    await invoke("hide_bar");
  } catch (e) {
    console.error("hide_bar failed", e);
  }
});

// ---------- Settings form ----------

function loadSettingsForm() {
  if (!config) return;
  ($("cfg-token") as HTMLInputElement).value = config.token;
  ($("cfg-base-url") as HTMLInputElement).value = config.base_url;
  ($("cfg-interval") as HTMLInputElement).value = String(config.interval_secs);
  ($("cfg-theme") as HTMLSelectElement).value =
    config.theme && THEMES.includes(config.theme) ? config.theme : "dark";
  ($("cfg-compact") as HTMLInputElement).checked = !!config.compact;
}

$("btn-save").addEventListener("click", async () => {
  const status = $("save-status");
  const newConfig: Config = {
    token: ($("cfg-token") as HTMLInputElement).value.trim(),
    base_url: ($("cfg-base-url") as HTMLInputElement).value.trim(),
    interval_secs: Number(($("cfg-interval") as HTMLInputElement).value) || 60,
    theme: ($("cfg-theme") as HTMLSelectElement).value,
    compact: ($("cfg-compact") as HTMLInputElement).checked,
    bar_visible: config?.bar_visible ?? true,
  };
  try {
    await invoke("save_config", { newConfig });
    config = newConfig;
    applyBodyClasses();
    await setExpanded(expanded); // resink ukuran window (compact <-> normal)
    status.textContent = "Tersimpan ✓";
    panelStatus.textContent = "Konfigurasi disimpan, data disegarkan.";
    setTimeout(() => (status.textContent = ""), 2500);
  } catch (e) {
    status.textContent = `Gagal: ${e}`;
  }
});

// ---------- Events dari backend ----------

listen<Snapshot>("stats://update", (ev) => {
  snapshot = ev.payload;
  renderAll();
});

listen("ui://open-settings", () => {
  if (!expanded) setExpanded(true);
  ($("settings") as HTMLDetailsElement).open = true;
  loadSettingsForm();
});

// Config berubah dari tray (compact/theme) atau save_config — sinkronkan UI.
listen<Config>("ui://config", (ev) => {
  config = ev.payload;
  applyBodyClasses();
  loadSettingsForm();
  void setExpanded(expanded); // resink ukuran window dengan mode aktif
});

// ---------- Init ----------

async function init() {
  try {
    const state = await invoke<{ config: Config; snapshot: Snapshot | null }>("get_state");
    config = state.config;
    snapshot = state.snapshot;
  } catch (e) {
    console.error("get_state failed", e);
  }
  applyBodyClasses();
  loadSettingsForm();
  renderAll();

  // countdown tiap detik cukup update slot reset + baris panel
  setInterval(() => {
    renderResetSlot();
    if (expanded) {
      const rows = panelRows.querySelectorAll(".row-value");
      // murah: render ulang panel tiap detik saat terbuka (isi kecil)
      renderPanel();
      void rows;
    }
  }, 1000);
}

init();
