import { invoke } from "@tauri-apps/api/core";

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
  hasModsFolder: boolean;
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

const $ = <T extends HTMLElement>(sel: string) => document.querySelector(sel) as T | null;

function log(lines: string | string[], clear = false) {
  const el = $<HTMLPreElement>("#log");
  if (!el) return;
  const text = Array.isArray(lines) ? lines.join("\n") : lines;
  el.textContent = clear ? text : `${el.textContent ? el.textContent + "\n" : ""}${text}`;
  el.scrollTop = el.scrollHeight;
}

function fmtBytes(n: number | null | undefined): string {
  if (n == null) return "—";
  const gb = n / 1_073_741_824;
  if (gb >= 1) return `${gb.toFixed(1)} GB`;
  const mb = n / 1_048_576;
  return `${mb.toFixed(0)} MB`;
}

function pathOverride(): string | null {
  const v = $<HTMLInputElement>("#game-path")?.value.trim();
  return v ? v : null;
}

async function refreshDisk() {
  const badge = $("#disk-badge");
  try {
    const disk = await invoke<DiskInfo>("get_disk_info", { path: pathOverride() });
    if (badge) {
      badge.innerHTML =
        disk.freeGb != null
          ? `<div class="muted">Free space</div><strong>${disk.freeGb.toFixed(1)} GB</strong>`
          : `<div class="muted">Free space</div><strong>—</strong>`;
    }
  } catch (e) {
    if (badge) badge.textContent = "Disk ?";
    console.error(e);
  }
}

async function refreshGame() {
  const status = $("#game-status");
  if (status) {
    status.className = "status muted";
    status.textContent = "Detecting…";
  }
  try {
    const info = await invoke<GameInfo>("get_game_info", { path: pathOverride() });
    if (!status) return;

    const lines = [
      info.found ? `✓ Found: ${info.gamePath}` : `✗ Not found: ${info.gamePath}`,
      info.betaKey ? `Steam beta: ${info.betaKey}` : "Steam beta: (none / default)",
      `A20-compatible: ${info.looksLikeA20 ? "yes" : "no / unknown"}`,
      `Size on disk: ${fmtBytes(info.sizeOnDiskBytes)}`,
      `BepInEx: ${info.hasBepinex ? "yes" : "no"} · run_bepinex.sh: ${info.hasRunBepinex ? "yes" : "no"} · Mods: ${info.hasModsFolder ? "yes" : "no"}`,
      ...info.notes.map((n) => `• ${n}`),
    ];
    status.textContent = lines.join("\n");
    status.className = `status ${
      !info.found ? "bad" : info.looksLikeA20 ? "ok" : "warn"
    }`;

    const input = $<HTMLInputElement>("#game-path");
    if (input && !input.value.trim()) {
      input.placeholder = info.gamePath;
    }
  } catch (e) {
    if (status) {
      status.className = "status bad";
      status.textContent = String(e);
    }
  }
}

async function installUl() {
  const btn = $<HTMLButtonElement>("#btn-install");
  const force = $<HTMLInputElement>("#force-install")?.checked ?? false;
  if (btn) btn.disabled = true;
  log("Installing Undead Legacy Experimental (in-place)…", true);
  try {
    const result = await invoke<InstallResult>("install_ul_experimental", {
      path: pathOverride(),
      force,
    });
    log([
      result.ok ? "SUCCESS" : "FAILED",
      result.message,
      result.downloadBytes != null ? `Download: ${fmtBytes(result.downloadBytes)}` : "",
      "",
      "Steps:",
      ...result.steps.map((s) => `  - ${s}`),
    ]);
    await refreshGame();
    await refreshDisk();
  } catch (e) {
    log(`Error: ${e}`);
  } finally {
    if (btn) btn.disabled = false;
  }
}

async function launchUl() {
  const btn = $<HTMLButtonElement>("#btn-launch");
  if (btn) btn.disabled = true;
  log("Launching…");
  try {
    const result = await invoke<LaunchResult>("launch_ul", { path: pathOverride() });
    log([result.ok ? "Launch OK" : "Launch failed", result.message, result.command && `cmd: ${result.command}`]);
  } catch (e) {
    log(`Error: ${e}`);
  } finally {
    if (btn) btn.disabled = false;
  }
}

window.addEventListener("DOMContentLoaded", () => {
  $("#btn-refresh")?.addEventListener("click", () => {
    void refreshGame();
    void refreshDisk();
  });
  $("#btn-install")?.addEventListener("click", () => void installUl());
  $("#btn-launch")?.addEventListener("click", () => void launchUl());
  $("#game-path")?.addEventListener("change", () => {
    void refreshGame();
    void refreshDisk();
  });
  void refreshGame();
  void refreshDisk();
});
