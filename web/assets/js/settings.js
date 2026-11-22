/* Preferencias persistentes: interfaz, arranque con Windows y bandeja. */

import { api } from "./api.js";
import { $, toast } from "./ui.js";

const STORAGE_KEY = "riftsense.settings.v1";
const DEFAULTS = Object.freeze({
  showAdvanced: false,
  startWithWindows: false,
  minimizeToTray: false,
});
const ADVANCED_TABS = new Set(["model", "data"]);

let current = { ...DEFAULTS };
let startupSupported = false;
let version = "";
let bound = false;

function normalize(payload) {
  return {
    showAdvanced: Boolean(payload?.showAdvanced),
    startWithWindows: Boolean(payload?.startWithWindows),
    minimizeToTray: Boolean(payload?.minimizeToTray),
  };
}

function localRead() {
  try {
    const raw = localStorage.getItem(STORAGE_KEY);
    return raw ? normalize(JSON.parse(raw)) : { ...DEFAULTS };
  } catch {
    return { ...DEFAULTS };
  }
}

function localWrite(settings) {
  try {
    localStorage.setItem(STORAGE_KEY, JSON.stringify(settings));
  } catch {
    /* almacenamiento no disponible: los ajustes siguen en memoria */
  }
}

function apply() {
  for (const [key, selector] of [
    ["showAdvanced", "#settings-advanced"],
    ["startWithWindows", "#settings-startup"],
    ["minimizeToTray", "#settings-tray"],
  ]) {
    const input = $(selector);
    if (input && input.checked !== current[key]) input.checked = current[key];
  }

  document.querySelectorAll("[data-advanced]").forEach((element) => {
    element.classList.toggle("hidden", !current.showAdvanced);
  });

  const startup = $("#settings-startup");
  if (startup) startup.disabled = !startupSupported;
  const note = $("#settings-startup-note");
  if (note) {
    note.textContent = startupSupported
      ? "Windows abrirá RiftSense minimizado en la bandeja al iniciar sesión."
      : "Abre la app de escritorio para activar el arranque con Windows y la bandeja.";
  }

  const versionBadge = $("#settings-version");
  if (versionBadge) versionBadge.textContent = version ? `Versión ${version}` : "RiftSense";
  const aboutVersion = $("#settings-about-version");
  if (aboutVersion) aboutVersion.textContent = version || "—";
  const aboutPatch = $("#settings-about-patch");
  if (aboutPatch) aboutPatch.textContent = $("#status-patch")?.textContent || "—";
  const aboutGames = $("#settings-about-games");
  if (aboutGames) aboutGames.textContent = $("#status-games")?.textContent || "—";

  window.dispatchEvent(
    new CustomEvent("riftsense:advanced", { detail: { visible: current.showAdvanced } })
  );
}

async function update(patch) {
  const previous = { ...current };
  current = { ...current, ...patch };
  apply();
  try {
    const saved = await api.post(
      "/api/settings",
      {
        showAdvanced: current.showAdvanced,
        startWithWindows: current.startWithWindows,
        minimizeToTray: current.minimizeToTray,
      },
      { timeout: 15000 }
    );
    current = normalize(saved);
    startupSupported = saved?.startupSupported === true;
    if (saved?.version) version = String(saved.version);
    localWrite(current);
    apply();
  } catch (err) {
    current = previous;
    apply();
    toast(err?.message || "No se pudieron guardar los ajustes.", "error", 6000);
  }
}

function bind() {
  $("#settings-advanced")?.addEventListener("change", (event) => {
    update({ showAdvanced: event.target.checked });
  });
  $("#settings-startup")?.addEventListener("change", (event) => {
    update({ startWithWindows: event.target.checked });
  });
  $("#settings-tray")?.addEventListener("change", (event) => {
    update({ minimizeToTray: event.target.checked });
  });
}

export async function init() {
  let remote = null;
  try {
    remote = await api.get("/api/settings", { timeout: 6000 });
  } catch (err) {
    console.debug("Ajustes del servidor no disponibles:", err?.message || err);
  }
  if (remote) {
    current = normalize(remote);
    startupSupported = remote.startupSupported === true;
    version = String(remote.version || "");
    localWrite(current);
  } else {
    current = localRead();
    startupSupported = false;
  }
  if (!bound) {
    bind();
    bound = true;
  }
  apply();
}

export function tabVisible(tab) {
  return !ADVANCED_TABS.has(tab) || current.showAdvanced;
}

export function refresh() {
  apply();
}
