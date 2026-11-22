/* Preferencias persistentes: interfaz, arranque con Windows y bandeja. */

import { api } from "./api.js";
import { state } from "./state.js";
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
  const dataNote = $("#settings-data-note");
  if (dataNote) {
    const end = state.meta?.dataset?.dateRange?.[1];
    const timestamp = end ? Date.parse(end) : NaN;
    if (Number.isFinite(timestamp)) {
      const days = Math.max(0, Math.floor((Date.now() - timestamp) / 86400000));
      dataNote.textContent =
        days > 45
          ? `Datos hasta ${end} (${days} días). Actualiza datos y modelos antes de confiar en las recomendaciones.`
          : `Datos hasta ${end} (${days} días). Dentro de la ventana de 45 días.`;
      dataNote.classList.toggle("bad", days > 45);
    } else {
      dataNote.textContent = "No se pudo leer la fecha del dataset.";
      dataNote.classList.add("bad");
    }
  }

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
  $("#settings-diagnostics")?.addEventListener("click", async () => {
    try {
      const data = await api.get("/api/diagnostics", { timeout: 15000 });
      const text = JSON.stringify(data, null, 2);
      if (navigator.clipboard?.writeText) {
        await navigator.clipboard.writeText(text);
        toast("Diagnóstico copiado al portapapeles.", "success");
      } else {
        console.info(text);
        toast("Diagnóstico escrito en la consola.", "info");
      }
    } catch (err) {
      toast(err?.message || "No se pudo generar el diagnóstico.", "error", 7000);
    }
  });
  $("#settings-logs")?.addEventListener("click", async () => {
    try {
      const result = await api.post("/api/diagnostics/open-logs", {}, { timeout: 15000 });
      toast(result?.directory ? `Logs: ${result.directory}` : "Carpeta de logs abierta.", "success", 7000);
    } catch (err) {
      toast(err?.message || "No se pudo abrir la carpeta de logs.", "error", 7000);
    }
  });
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
