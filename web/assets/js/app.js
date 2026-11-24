/* Bootstrap de la SPA: navegación, estado global y carga inicial. */

import { api } from "./api.js";
import { hydrateMeta, meta, state } from "./state.js";
import { $, $$, clearToasts, num, toast } from "./ui.js";
import * as draft from "./draft.js";
import * as live from "./live.js";
import * as client from "./client.js";
import * as summary from "./summary.js";
import * as model from "./model.js";
import * as data from "./data.js";
import * as settings from "./settings.js";

const TABS = ["draft", "live", "summary", "client", "model", "data", "settings"];
const TAB_LABELS = { draft: "Preparación", live: "En partida", summary: "Tu historial", client: "Conexión local", model: "Modelos", data: "Explorador de datos", settings: "Preferencias" };
const ONBOARDING_KEY = "riftsense.onboarding.v1";
let booted = false;

function rememberOnboarding() {
  try {
    localStorage.setItem(ONBOARDING_KEY, "done");
  } catch {
    /* almacenamiento no disponible */
  }
}

function openOnboarding(force = false) {
  const dialog = $("#onboarding");
  if (!dialog) return;
  try {
    if (!force && localStorage.getItem(ONBOARDING_KEY)) return;
  } catch {
    /* sin almacenamiento: se muestra igualmente */
  }
  if (!dialog.open) dialog.showModal();
}

function updateStatus(payload, health) {
  const dot = $("#status-dot");
  const text = $("#status-text");
  const patch = $("#status-patch");
  const games = $("#status-games");
  const models = $("#status-models");
  const data = $("#status-data");
  const dataset = payload?.dataset || health?.dataset || {};
  const draftReady = payload?.models?.draft ?? false;
  const buildReady = payload?.models?.build ?? false;
  const count = Number(draftReady) + Number(buildReady);

  if (patch) patch.textContent = payload?.patch || "—";
  if (games) games.textContent = dataset.games ? num(dataset.games) : "—";
  if (models) models.textContent = `${count}/2`;
  const end = dataset.dateRange?.[1];
  if (data) data.textContent = end || "—";
  const days = end ? Math.floor((Date.now() - Date.parse(end)) / 86400000) : NaN;
  if (Number.isFinite(days) && days > 45) {
    toast(`Los datos tienen ${days} días. Actualiza el dataset y los modelos para evitar recomendaciones desactualizadas.`, "info", 9000);
  }
  if (dot) dot.className = "status-dot ok";
  if (text) text.textContent = count === 2 ? "Motor neuronal listo" : count === 1 ? "Modelo parcial" : "Modo datos pro";
}

function setTab(tab, updateHash = true) {
  const requested = TABS.includes(tab) ? tab : "draft";
  tab = settings.tabVisible(requested) ? requested : "settings";
  state.activeTab = tab;
  document.body.dataset.view = tab;
  const sectionLabel = $("#workspace-section");
  if (sectionLabel) sectionLabel.textContent = TAB_LABELS[tab];
  clearToasts();
  for (const name of TABS) {
    $(`#view-${name}`)?.classList.toggle("hidden", name !== tab);
  }
  $$(".nav-item").forEach((button) => {
    const active = button.dataset.tab === tab;
    button.classList.toggle("active", active);
    button.setAttribute("aria-current", active ? "page" : "false");
  });
  if ((updateHash || requested !== tab) && location.hash !== `#${tab}`) {
    history.replaceState(null, "", `#${tab}`);
  }
  if (tab === "summary") summary.refresh();
  if (tab === "model") model.load();
  if (tab === "data") data.load();
  if (tab === "settings") settings.refresh();
  client.setActive(tab === "client");
  const reduceMotion = window.matchMedia?.("(prefers-reduced-motion: reduce)").matches;
  window.scrollTo({ top: 0, behavior: reduceMotion ? "auto" : "smooth" });
}

function bindNav() {
  $("#workspace-help")?.addEventListener("click", () => openOnboarding(true));
  $(".skip-link")?.addEventListener("click", (event) => {
    // #main no es una ruta: saltar al contenido no debe cambiar de vista.
    event.preventDefault();
    $("#main")?.focus({ preventScroll: true });
    window.scrollTo({ top: 0, behavior: "auto" });
  });
  $$(".nav-item").forEach((button) => {
    button.addEventListener("click", () => setTab(button.dataset.tab));
  });
  window.addEventListener("hashchange", () => {
    setTab(location.hash.replace("#", "") || "draft", false);
  });
}

async function checkForUpdates(manual = false) {
  try {
    const info = await api.get("/api/update/check", { timeout: 15000 });
    if (!info?.available) {
      if (manual) {
        if (info?.error) {
          toast(`No se pudo comprobar: ${info.error}`, "error", 7000);
        } else {
          toast(`Tienes la última versión (${info?.current || "actual"}).`, "info");
        }
      }
      return;
    }
    const banner = $("#update-banner");
    const title = $("#update-title");
    const detail = $("#update-detail");
    const button = $("#update-install");
    const dismiss = $("#update-dismiss");
    if (!banner || !button) return;
    if (title) title.textContent = `RiftSense ${info.latest} disponible`;
    if (detail) {
      detail.textContent = `Tienes la ${info.current}. Pulsa actualizar para descargar e instalar.`;
    }
    button.textContent = `Actualizar a ${info.latest}`;
    button.onclick = async () => {
      button.disabled = true;
      button.textContent = "Descargando…";
      try {
        await api.post("/api/update/install", {}, { timeout: 300000 });
        toast("Instalando actualización. RiftSense se reiniciará.", "info", 12000);
      } catch (err) {
        button.disabled = false;
        button.textContent = `Actualizar a ${info.latest}`;
        toast(err.message || "No se pudo instalar la actualización.", "error", 8000);
      }
    };
    if (dismiss) dismiss.onclick = () => banner.classList.add("hidden");
    banner.classList.remove("hidden");
  } catch (err) {
    if (manual) {
      toast(err?.message || "No se pudo comprobar si hay actualizaciones.", "error", 7000);
    } else {
      console.debug("Comprobación de actualizaciones no disponible:", err?.message || err);
    }
  }
}

function fatal(err) {
  const dot = $("#status-dot");
  const text = $("#status-text");
  if (dot) dot.className = "status-dot bad";
  if (text) text.textContent = "Motor no disponible";
  const main = $("#main");
  if (main) {
    const banner = document.createElement("div");
    banner.className = "card";
    banner.style.borderColor = "rgba(255, 95, 109, .4)";
    banner.innerHTML = `<div class="card-head"><div><h2>El motor aún no está listo</h2>
      <p>${String(err.message || err).replace(/</g, "&lt;")}</p></div></div>
      <p class="fineprint">Genera los datos y modelos desde la carpeta del proyecto:</p>
      <div class="pipeline">
        <code>python -m lolanalyzer.scraper.leaguepedia --since 2024-01-01</code>
        <code>python -m lolanalyzer.data.build_dataset</code>
        <code>python -m lolanalyzer.models.train_draft</code>
        <code>python -m lolanalyzer.models.train_build</code>
      </div>
      <p class="fineprint">Después recarga esta página.</p>`;
    main.prepend(banner);
  }
  toast("No se pudo conectar con el motor de RiftSense.", "error", 8000);
}

async function boot() {
  if (booted) return;
  booted = true;
  window.addEventListener("unhandledrejection", (event) => {
    console.error(event.reason);
    toast(event.reason?.message || "Error inesperado en la interfaz.", "error");
  });
  bindNav();
  window.addEventListener("riftsense:advanced", (event) => {
    if (!event.detail?.visible && ["model", "data"].includes(state.activeTab)) {
      setTab("draft");
    }
  });
  try {
    const [metaPayload, health] = await Promise.all([
      api.get("/api/meta"),
      api.get("/api/health").catch(() => null),
    ]);
    hydrateMeta(metaPayload);
    updateStatus(metaPayload, health);
    draft.init();
    live.init();
    summary.init();
    data.initEvents();
    client.init();
    await settings.init();
    $("#settings-update")?.addEventListener("click", () => checkForUpdates(true));
    $("#settings-help")?.addEventListener("click", () => openOnboarding(true));
    $("#onboarding")?.addEventListener("close", rememberOnboarding);
    $("#onboarding-start")?.addEventListener("click", () => {
      rememberOnboarding();
      $("#onboarding")?.close();
    });
    $("#onboarding-settings")?.addEventListener("click", () => {
      rememberOnboarding();
      $("#onboarding")?.close();
      setTab("settings");
    });
    setTab(location.hash.replace("#", "") || "draft", false);
    if (!localStorage.getItem(ONBOARDING_KEY)) {
      setTimeout(() => openOnboarding(false), 900);
    }
    if (!metaPayload.models?.draft || !metaPayload.models?.build) {
      toast("Algún modelo no está disponible: se usarán estadísticas pro.", "info", 6000);
    }
    checkForUpdates();
  } catch (err) {
    fatal(err);
  }
}

boot();
