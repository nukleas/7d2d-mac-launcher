import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";

type GameInfo = {
  found: boolean;
  gamePath: string;
  appPath: string;
  manifestPath: string;
  betaKey: string | null;
  name: string | null;
  sizeOnDiskBytes: number | null;
  looksLikeA20: boolean;
  hasBepinex: boolean;
  hasRunBepinex: boolean;
  hasDoorstop: boolean;
  hasModsFolder: boolean;
  modReady: boolean;
  notes: string[];
};

type DiskInfo = {
  path: string;
  freeBytes: number | null;
  freeGb: number | null;
};

type InstallResult = {
  ok: boolean;
  message: string;
  gamePath: string;
  steps: string[];
  downloadBytes: number | null;
};

type LaunchResult = {
  ok: boolean;
  message: string;
  command: string;
};

type ProgressEvent = {
  stage: string;
  title: string;
  detail: string;
  percent: number;
  bytesDone: number | null;
  bytesTotal: number | null;
  indeterminate: boolean;
};

const $ = <T extends HTMLElement>(sel: string) => document.querySelector(sel) as T | null;

let installing = false;
let unlistenProgress: UnlistenFn | null = null;

function pathOverride(): string | null {
  const v = $<HTMLInputElement>("#game-path")?.value.trim();
  return v ? v : null;
}

function setStep(active: 1 | 2 | 3, doneUpTo = 0) {
  document.querySelectorAll<HTMLElement>(".step").forEach((el) => {
    const n = Number(el.dataset.step);
    el.classList.toggle("is-done", n <= doneUpTo);
    if (n === active) el.setAttribute("aria-current", "step");
    else el.removeAttribute("aria-current");
  });
}

function setReady(state: "busy" | "ok" | "warn" | "bad", title: string, sub: string) {
  const banner = $("#ready-banner");
  const t = $("#ready-title");
  const s = $("#ready-sub");
  if (banner) {
    banner.classList.remove("is-ok", "is-warn", "is-bad", "is-busy");
    banner.classList.add(`is-${state}`);
  }
  if (t) t.textContent = title;
  if (s) s.textContent = sub;
}

function setChips(info: GameInfo, freeGb: number | null) {
  const host = $("#status-chips");
  if (!host) return;
  const chips: { text: string; cls: string }[] = [];
  chips.push({
    text: info.found ? "Game found" : "Game missing",
    cls: info.found ? "ok" : "bad",
  });
  chips.push({
    text: info.betaKey ? `Beta: ${info.betaKey}` : "Beta: default",
    cls: info.looksLikeA20 ? "ok" : info.found ? "warn" : "bad",
  });
  chips.push({
    text: info.looksLikeA20 ? "A20.7 ready" : "Needs alpha20.7",
    cls: info.looksLikeA20 ? "ok" : "warn",
  });
  if (freeGb != null) {
    chips.push({
      text: `${freeGb.toFixed(1)} GB free`,
      cls: freeGb >= 2 ? "ok" : "bad",
    });
  }
  if (info.modReady) {
    chips.push({ text: "Mod ready to play", cls: "ok" });
  } else if (info.hasBepinex && !info.hasDoorstop) {
    chips.push({ text: "Mod incomplete — reinstall", cls: "warn" });
  } else if (info.hasBepinex || info.hasRunBepinex) {
    chips.push({ text: "Mod partially installed", cls: "warn" });
  }
  host.innerHTML = chips.map((c) => `<span class="chip ${c.cls}">${c.text}</span>`).join("");
}

function setChecklist(info: GameInfo, freeGb: number | null) {
  const steam = document.querySelector<HTMLElement>('[data-req="steam"]');
  const beta = document.querySelector<HTMLElement>('[data-req="beta"]');
  const space = document.querySelector<HTMLElement>('[data-req="space"]');
  const mark = (el: HTMLElement | null, ok: boolean) => {
    if (!el) return;
    el.classList.toggle("ok", ok);
    el.classList.toggle("bad", !ok);
  };
  mark(steam, info.found);
  mark(beta, info.looksLikeA20);
  mark(space, freeGb == null ? true : freeGb >= 2);
}

function setProgressVisible(show: boolean) {
  const card = $("#progress-card");
  if (card) card.hidden = !show;
}

function applyProgress(p: ProgressEvent) {
  setProgressVisible(true);
  const card = $("#progress-card");
  const track = $("#progress-track");
  const bar = $("#progress-bar");
  const title = $("#progress-title");
  const detail = $("#progress-detail");
  const pct = $("#progress-pct");

  if (title) title.textContent = p.title;
  if (detail) detail.textContent = p.detail;
  if (pct) pct.textContent = p.indeterminate ? "…" : `${p.percent}%`;

  if (track && bar) {
    track.classList.toggle("cd-progress--indeterminate", p.indeterminate);
    track.classList.remove("cd-progress--success", "cd-progress--danger");
    if (p.stage === "finish") track.classList.add("cd-progress--success");
    if (p.stage === "error") track.classList.add("cd-progress--danger");
    bar.style.width = `${Math.max(0, Math.min(100, p.percent))}%`;
    track.setAttribute("aria-valuenow", String(p.percent));
  }

  if (card) {
    card.classList.toggle("is-error", p.stage === "error");
    card.classList.toggle("is-done", p.stage === "finish");
  }

  const order = ["check", "download", "extract", "copy", "finish"];
  const idx = order.indexOf(p.stage === "error" ? "check" : p.stage);
  document.querySelectorAll<HTMLElement>("#stage-list li").forEach((li) => {
    const stage = li.dataset.stage || "";
    const si = order.indexOf(stage);
    li.classList.remove("active", "done", "error");
    if (p.stage === "error" && stage === "check") li.classList.add("error");
    else if (si < idx || p.stage === "finish") li.classList.add("done");
    else if (si === idx) li.classList.add("active");
  });

  if (p.stage === "download" || p.stage === "extract" || p.stage === "copy") {
    setStep(2, 1);
  } else if (p.stage === "finish") {
    setStep(3, 2);
  } else if (p.stage === "check") {
    setStep(1, 0);
  }
}

function showToast(ok: boolean, message: string) {
  const el = $("#result-toast");
  if (!el) return;
  el.hidden = false;
  el.className = `toast ${ok ? "ok" : "bad"}`;
  el.textContent = message;
}

function setBusy(busy: boolean) {
  installing = busy;
  document.body.classList.toggle("is-installing", busy);
  const install = $<HTMLButtonElement>("#btn-install");
  const launch = $<HTMLButtonElement>("#btn-launch");
  const refresh = $<HTMLButtonElement>("#btn-refresh");
  if (install) {
    install.disabled = busy;
    install.textContent = busy ? "Installing… (window stays usable)" : "Install Undead Legacy";
  }
  if (refresh) refresh.disabled = busy;
  if (launch) launch.disabled = busy || launch.disabled;
}

async function refreshDisk(): Promise<number | null> {
  try {
    const disk = await invoke<DiskInfo>("get_disk_info", { path: pathOverride() });
    const el = $("#stat-disk .stat-value");
    if (el) {
      el.textContent = disk.freeGb != null ? `${disk.freeGb.toFixed(1)} GB` : "—";
    }
    return disk.freeGb;
  } catch {
    const el = $("#stat-disk .stat-value");
    if (el) el.textContent = "?";
    return null;
  }
}

async function refreshGame() {
  setReady("busy", "Looking for 7 Days to Die…", "Checking your Steam library");
  setStep(1, 0);
  try {
    const [info, freeGb] = await Promise.all([
      invoke<GameInfo>("get_game_info", { path: pathOverride() }),
      refreshDisk(),
    ]);
    const betaEl = $("#stat-beta .stat-value");
    if (betaEl) betaEl.textContent = info.betaKey || "default";

    setChips(info, freeGb);
    setChecklist(info, freeGb);

    const launch = $<HTMLButtonElement>("#btn-launch");
    const install = $<HTMLButtonElement>("#btn-install");

    if (!info.found) {
      setReady(
        "bad",
        "We couldn’t find the game",
        "Install 7 Days to Die from Steam, then hit Refresh.",
      );
      if (launch) launch.disabled = true;
      if (install) install.disabled = true;
      setStep(1, 0);
      return;
    }

    if (!info.looksLikeA20) {
      setReady(
        "warn",
        "Game found — needs Alpha 20.7",
        "In Steam: right-click 7 Days to Die → Properties → Betas → choose alpha20.7, wait for it to finish updating, then Refresh.",
      );
      if (launch) launch.disabled = !(info.hasBepinex || info.hasRunBepinex);
      if (install) install.disabled = false;
      setStep(1, 0);
      return;
    }

    if (info.modReady) {
      setReady(
        "ok",
        "Ready to play!",
        "Everything looks installed. Press Play (use this app every time — not Steam’s Play button).",
      );
      if (launch) launch.disabled = false;
      if (install) install.disabled = false;
      setStep(3, 2);
    } else if (info.hasBepinex && !info.hasDoorstop) {
      setReady(
        "warn",
        "Install is incomplete",
        "A required launch file is missing. Press Install again to repair (it’s safe).",
      );
      if (launch) launch.disabled = true;
      if (install) install.disabled = false;
      setStep(2, 1);
    } else {
      setReady(
        "ok",
        "Game looks good",
        "Steam beta looks right. Next: press Install Undead Legacy (it won’t copy the whole game).",
      );
      if (launch) launch.disabled = true;
      if (install) install.disabled = false;
      setStep(2, 1);
    }

    const input = $<HTMLInputElement>("#game-path");
    if (input && !input.value.trim()) input.placeholder = info.gamePath;
  } catch (e) {
    setReady("bad", "Something went wrong while checking", String(e));
  }
}

async function installUl() {
  if (installing) return;
  setBusy(true);
  setStep(2, 1);
  setProgressVisible(true);
  applyProgress({
    stage: "check",
    title: "Starting install…",
    detail: "Hang tight — progress will update below",
    percent: 1,
    bytesDone: null,
    bytesTotal: null,
    indeterminate: true,
  });
  showToast(true, "Installing… this can take several minutes. Please leave this window open.");

  const force = $<HTMLInputElement>("#force-install")?.checked ?? false;

  try {
    const result = await invoke<InstallResult>("install_ul_experimental", {
      path: pathOverride(),
      force,
    });
    showToast(result.ok, result.message);
    if (result.ok) {
      applyProgress({
        stage: "finish",
        title: "All set!",
        detail: "Press Play when you’re ready",
        percent: 100,
        bytesDone: result.downloadBytes,
        bytesTotal: result.downloadBytes,
        indeterminate: false,
      });
      setStep(3, 2);
    }
    await refreshGame();
  } catch (e) {
    applyProgress({
      stage: "error",
      title: "Install failed",
      detail: String(e),
      percent: 0,
      bytesDone: null,
      bytesTotal: null,
      indeterminate: false,
    });
    showToast(false, String(e));
  } finally {
    setBusy(false);
    await refreshGame();
  }
}

async function launchUl() {
  const btn = $<HTMLButtonElement>("#btn-launch");
  const note = $("#launch-note");
  if (btn) btn.disabled = true;
  if (note) note.textContent = "Starting…";
  try {
    const result = await invoke<LaunchResult>("launch_ul", { path: pathOverride() });
    if (note) note.textContent = result.message;
    showToast(result.ok, result.message);
  } catch (e) {
    if (note) note.textContent = String(e);
    showToast(false, String(e));
  } finally {
    if (btn) btn.disabled = false;
  }
}

window.addEventListener("DOMContentLoaded", () => {
  $("#btn-refresh")?.addEventListener("click", () => void refreshGame());
  $("#btn-install")?.addEventListener("click", () => void installUl());
  $("#btn-launch")?.addEventListener("click", () => void launchUl());
  $("#game-path")?.addEventListener("change", () => void refreshGame());

  void listen<ProgressEvent>("install-progress", (event) => {
    applyProgress(event.payload);
  }).then((un) => {
    unlistenProgress = un;
  });

  window.addEventListener("beforeunload", () => {
    unlistenProgress?.();
  });

  void refreshGame();
});
