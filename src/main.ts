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
  gameLine: "a20" | "v26" | "unsupported" | "updating" | "unknown";
  suggestedChannel: Channel | null;
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

type Channel = "stable" | "experimental";

/// Kept in step with channel.rs — the Rust side owns the real requirements.
const CHANNELS: Record<Channel, { ul: string; game: string; branch: string; freeGb: number }> = {
  stable: {
    ul: "Undead Legacy 2.6 (stable)",
    game: "Alpha 20.7",
    branch: "alpha20.7",
    freeGb: 5,
  },
  experimental: {
    ul: "Undead Legacy 2.7 (experimental)",
    game: "v2.6",
    branch: "v2.6",
    freeGb: 14,
  },
};

const $ = <T extends HTMLElement>(sel: string) => document.querySelector(sel) as T | null;

/// What the user picked, or the channel matching the installed game.
let channel: Channel = "experimental";
let channelLocked = false;

function currentChannel(): Channel {
  return channel;
}

function channelMatchesGame(info: GameInfo): boolean {
  return info.suggestedChannel === currentChannel();
}

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
  const spec = CHANNELS[currentChannel()];
  const matches = channelMatchesGame(info);
  chips.push({
    text: info.betaKey ? `Beta: ${info.betaKey}` : "Beta: default",
    cls: matches ? "ok" : info.found ? "warn" : "bad",
  });
  chips.push({
    text: matches ? `${spec.game} ready` : `Needs game ${spec.game}`,
    cls: matches ? "ok" : "warn",
  });
  if (freeGb != null) {
    chips.push({
      text: `${freeGb.toFixed(1)} GB free`,
      cls: freeGb >= spec.freeGb ? "ok" : "bad",
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
  const spec = CHANNELS[currentChannel()];
  if (beta) beta.innerHTML = `Steam beta set to <strong>${spec.branch}</strong>`;
  if (space) space.textContent = `About ${spec.freeGb} GB free space to download and unpack`;
  mark(steam, info.found);
  mark(beta, channelMatchesGame(info));
  mark(space, freeGb == null ? true : freeGb >= spec.freeGb);
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
    if (!channelLocked && info.suggestedChannel && info.suggestedChannel !== channel) {
      channel = info.suggestedChannel;
      const radio = document.querySelector<HTMLInputElement>(
        `input[name="ul-channel"][value="${channel}"]`,
      );
      if (radio) radio.checked = true;
    }

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

    if (!channelMatchesGame(info)) {
      const spec = CHANNELS[currentChannel()];
      if (info.gameLine === "updating") {
        // Steam flips the branch instantly but the files take a long download.
        // Installing in between puts the mod on the wrong game generation.
        setReady(
          "warn",
          "Steam is still downloading the game",
          `The ${spec.branch} branch is selected, but the game files on disk are still the old version. Wait for Steam to finish, then press Check again.`,
        );
      } else {
        setReady(
          "warn",
          `Game found — ${spec.ul} needs game ${spec.game}`,
          `In Steam: right-click 7 Days to Die → Properties → Betas → ${spec.branch}, wait for it to finish updating, then Refresh.`,
        );
      }
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
    const result = await invoke<InstallResult>("install_ul", {
      channel: currentChannel(),
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
  document.querySelectorAll<HTMLInputElement>('input[name="ul-channel"]').forEach((radio) => {
    radio.addEventListener("change", () => {
      if (!radio.checked) return;
      channel = radio.value as Channel;
      // An explicit pick sticks, even if it disagrees with what is installed.
      channelLocked = true;
      void refreshGame();
    });
  });

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
