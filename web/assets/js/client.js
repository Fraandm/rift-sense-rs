/* Vista del cliente local + sincronizacion con Draft y En vivo. */

import { api } from "./api.js";
import { meta, state, championByName, roleLabel, ROLE_SHORT } from "./state.js";
import { $, avatarHTML, emptyState, esc, num, toast } from "./ui.js";
import { applyClientDraft } from "./draft.js";
import { applyClientState } from "./live.js";

const PHASE_LABELS = {
  None: "Cliente abierto",
  Lobby: "Lobby",
  Matchmaking: "Buscando partida",
  ReadyCheck: "Ready check",
  ChampSelect: "Champ select",
  GameStart: "Iniciando partida",
  InProgress: "Partida en curso",
  Reconnect: "Reconectando",
  WaitingForStats: "Estadísticas",
  PreEndOfGame: "Fin de partida",
  EndOfGame: "Fin de partida",
  TerminatedInError: "Error",
};

const LOCATION_SOURCES = {
  env: "Variable de entorno",
  saved: "Ruta guardada",
  "riot-metadata": "Metadata de Riot",
  "riot-installs": "Riot Client",
  registry: "Registro de Windows",
  common: "Ruta habitual",
  scan: "Búsqueda en discos",
  manual: "Ruta manual",
};

let status = null;
let draftData = null;
let liveData = null;
let auto = true;
let active = false;
let timer = null;
let busy = false;
let lastDraftSig = null;
let lastConnected = null;
let initialized = false;

function phaseLabel(phase) {
  return PHASE_LABELS[phase] || phase || "Sin fase";
}

function clock(minutes) {
  const total = Math.max(0, Math.round((Number(minutes) || 0) * 60));
  return `${Math.floor(total / 60)}:${String(total % 60).padStart(2, "0")}`;
}

/* ------------------------------------------------------------------ */
/* Render                                                              */
/* ------------------------------------------------------------------ */
function setBadge(selector, text, tone = "") {
  const el = $(selector);
  if (!el) return;
  el.className = `badge ${tone}`.trim();
  el.textContent = text;
}

function renderLocation(search) {
  const card = $("#client-locate");
  const dot = $("#client-locate-dot");
  const title = $("#client-locate-title");
  const desc = $("#client-locate-desc");
  const path = $("#client-locate-path");
  const button = $("#client-search");
  if (!card) return;

  const searching = Boolean(search?.searching);
  const found = Boolean(search?.installPath);
  card.classList.toggle("searching", searching);
  card.classList.toggle("found", !searching && found);
  card.classList.toggle("missing", !searching && !found);
  if (dot) dot.className = `status-dot ${searching ? "" : found ? "ok" : "bad"}`.trim();
  if (button) button.disabled = searching;

  if (searching) {
    const drives = (search.drives || []).join(" · ") || "unidades del sistema";
    const current = search.currentDrive ? ` · ${search.currentDrive}` : "";
    if (title) title.textContent = "Buscando en los discos…";
    if (desc) {
      desc.textContent = `${drives}${current} · ${num(search.scannedDirs || 0)} carpetas analizadas`;
    }
    if (path) path.textContent = "Buscando LeagueClient.exe / lockfile…";
    return;
  }

  if (found) {
    const drive = search.drive || "?";
    const source = LOCATION_SOURCES[search.source] || search.source || "búsqueda";
    const lock = search.lockfilePath
      ? "lockfile disponible"
      : "cliente cerrado: falta el lockfile";
    if (title) title.textContent = `Cliente encontrado en el disco ${drive}`;
    if (desc) desc.textContent = `Fuente: ${source} · ${lock}`;
    if (path) path.textContent = search.installPath;
    const input = $("#client-path");
    if (input && document.activeElement !== input && !input.dataset.dirty) {
      input.value = search.installPath;
    }
    return;
  }

  if (title) title.textContent = "Cliente no encontrado";
  if (desc) {
    desc.textContent =
      search?.error || "Revisa que League of Legends esté instalado o fija la ruta manualmente.";
  }
  if (path) path.textContent = "—";
}

function renderStatus() {
  const dot = $("#client-dot");
  const title = $("#client-state-title");
  const desc = $("#client-state-desc");
  const pills = $("#client-pills");
  const connected = Boolean(status?.connected || status?.liveClient);

  if (dot) dot.className = `status-dot ${connected ? "ok" : "bad"}`;

  let heading = "Cliente no detectado";
  let message = status?.error || "Abre League of Legends en este mismo PC para leer el champ select y la partida.";
  if (status?.liveClient) {
    heading = "Partida en curso";
    message = "Live Client Data disponible: campeón, objetos, oro, minuto y marcador. Solo lectura.";
  } else if (status?.champSelect) {
    heading = "Champ select detectado";
    message = "Picks, bans y rol en tiempo real. Puedes importar el draft o activar la sincronización automática.";
  } else if (status?.inGame || status?.phase === "InProgress") {
    heading = "Partida en curso";
    message = "Esperando al Live Client Data API del juego. Comprueba que la partida está cargada.";
  } else if (status?.connected) {
    heading = "Cliente abierto";
    message =
      status.phase === "None" || !status.phase
        ? "El cliente está abierto. Entra en champ select o en una partida para leer datos."
        : `Fase: ${phaseLabel(status.phase)}. Entra en champ select o en una partida para leer datos.`;
  } else if (status?.search?.searching) {
    heading = "Buscando cliente…";
    message = "Analizando unidades, registro y metadata de Riot.";
  } else if (status?.search?.installPath) {
    heading = "Cliente cerrado";
    message =
      status.error ||
      `Encontrado en el disco ${status.search.drive || "?"}, pero no está abierto. Abre League of Legends.`;
  }

  if (title) title.textContent = heading;
  if (desc) desc.textContent = message;

  if (pills) {
    const items = [];
    items.push(`<span class="pill">${esc(phaseLabel(status?.phase))}</span>`);
    if (status?.summoner) items.push(`<span class="pill">${esc(status.summoner)}</span>`);
    items.push(
      `<span class="pill ${status?.connected ? "teal" : ""}">LCU ${
        status?.connected ? "conectado" : "no detectado"
      }</span>`
    );
    items.push(
      `<span class="pill ${status?.liveClient ? "teal" : ""}">Live API ${
        status?.liveClient ? "lista" : "en espera"
      }</span>`
    );
    if (status?.search?.drive) {
      items.push(`<span class="pill teal">Disco ${esc(status.search.drive)}</span>`);
    }
    if (status?.search?.searching) {
      items.push(`<span class="pill">Buscando…</span>`);
    }
    if (status?.lockfilePath) {
      items.push(`<span class="pill" title="${esc(status.lockfilePath)}">Lockfile OK</span>`);
    }
    pills.innerHTML = items.join("");
  }

  renderLocation(status?.search);

  setBadge("#client-draft-badge", status?.champSelect ? "En vivo" : "—", status?.champSelect ? "teal" : "");
  setBadge("#client-live-badge", status?.liveClient ? "En partida" : "—", status?.liveClient ? "teal" : "");

  const useDraft = $("#client-use-draft");
  const classicDraft = (draftData?.gameMode || "classic") === "classic";
  if (useDraft) useDraft.disabled = !(status?.champSelect && draftData?.available && classicDraft);
  const useLive = $("#client-use-live");
  if (useLive) useLive.disabled = !(status?.liveClient && liveData?.available);

  const navDot = $("#client-nav-dot");
  if (navDot) {
    navDot.classList.toggle("ok", connected);
    navDot.classList.toggle("found", !connected && Boolean(status?.search?.installPath));
  }

  const autoCheck = $("#client-auto");
  if (autoCheck) autoCheck.checked = auto;
  const liveAuto = $("#live-auto");
  if (liveAuto) {
    liveAuto.textContent = `Auto: ${auto ? "on" : "off"}`;
    liveAuto.classList.toggle("auto", auto);
  }
  const note = $("#client-auto-note");
  if (note) {
    note.textContent = auto
      ? "Sincronización activa: actualiza el draft en champ select y la build durante la partida."
      : "Cada 3 segundos mientras la aplicación esté visible. No envía inputs al juego.";
  }
}

function teamBlock(title, side, list, tone = "", classic = true) {
  const rows = meta.roles
    .map((role, index) => {
      const name = Array.isArray(list) ? list[index] : null;
      const champ = name ? championByName(name) : null;
      return `<div class="client-player-row">
        <span class="role">${classic ? ROLE_SHORT[role] : index + 1}</span>
        ${champ ? avatarHTML(champ, "sm") : `<span class="avatar sm" data-initial="—" aria-hidden="true"></span>`}
        <strong class="${champ ? "" : "missing"}">${esc(champ?.name || "Sin pick")}</strong>
      </div>`;
    })
    .join("");
  return `<div class="client-team"><h3>${esc(title)}<span class="${tone}">${esc(side)}</span></h3>${rows}</div>`;
}

function renderDraft(message = "") {
  const host = $("#client-draft");
  if (!host) return;
  const data = draftData;
  if (!data?.available) {
    host.innerHTML = emptyState(
      "i-plug",
      "Sin champ select",
      message || "Entra en una partida y elige campeón para importar picks y bans."
    );
    setBadge("#client-draft-badge", "—");
    const button = $("#client-use-draft");
    if (button) button.disabled = true;
    return;
  }

  const classic = (data.gameMode || "classic") === "classic";
  const allies = teamBlock(
    data.gameMode === "arena" ? "Tu trío" : "Tu equipo",
    data.side === "red" ? "Rojo" : "Azul",
    data.allies,
    "",
    classic
  );
  const enemies = teamBlock(
    classic ? "Rival" : "Rivales",
    data.side === "red" ? "Azul" : "Rojo",
    data.enemies,
    "red",
    classic
  );
  const bans = (data.bans || [])
    .map((name) => championByName(name))
    .filter(Boolean)
    .map((champ) => `<span class="ban-chip">${avatarHTML(champ, "sm")}<span>${esc(champ.name)}</span></span>`)
    .join("");
  const timerText =
    data.timerSeconds != null ? `${Math.round(data.timerSeconds)} s restantes` : "timer no disponible";
  const myChampion = data.myChampion ? `Tu campeón: ${esc(data.myChampion)}` : "Sin campeón seleccionado";

  host.innerHTML = `<div class="client-team-grid">${allies}${enemies}</div>
    <div class="client-bans"><span class="label">Bans</span>${
      bans || `<span class="ban-empty">Sin bans</span>`
    }</div>
    <p class="fineprint">${myChampion} · ${esc(timerText)} · ${esc(
      phaseLabel(data.phase)
    )} · ${esc(data.modeLabel || "")}${classic ? " · Tu slot queda libre en Draft para comparar." : " · En este modo no hay draft de Grieta; usa En vivo para la build."}</p>`;

  setBadge("#client-draft-badge", "En vivo", "teal");
  const button = $("#client-use-draft");
  if (button) button.disabled = !classic;
}

function itemChip(item) {
  const name = item.name || `Objeto ${item.id}`;
  return `<span class="client-item">${avatarHTML(
    { id: item.id, name },
    "",
    "item"
  )}<span>${esc(name)}</span></span>`;
}

function renderLive(message = "") {
  const host = $("#client-live");
  if (!host) return;
  const data = liveData;
  if (!data?.available) {
    host.innerHTML = emptyState(
      "i-live",
      "Sin partida en curso",
      message || "El Live Client Data API responde en el puerto 2999 cuando estás dentro de una partida."
    );
    setBadge("#client-live-badge", "—");
    const button = $("#client-use-live");
    if (button) button.disabled = true;
    return;
  }

  const champ = championByName(data.champion);
  const classic = (data.mode || data.gameMode || "classic") === "classic";
  const allies = teamBlock("Tu equipo", data.side === "red" ? "Rojo" : "Azul", data.allies, "", classic);
  const enemies = teamBlock(
    classic ? "Rival" : "Rivales",
    data.side === "red" ? "Azul" : "Rojo",
    data.enemies,
    "red",
    classic
  );
  const items = (data.items || []).map(itemChip).join("") || `<span class="ban-empty">Sin objetos comprados</span>`;
  const scores = data.scores || {};

  host.innerHTML = `
    <div class="client-live-head">
      ${champ ? avatarHTML(champ, "lg") : `<span class="avatar lg" data-initial="?" aria-hidden="true"></span>`}
      <div>
        <h3>${esc(data.champion || "Campeón")}</h3>
        <p>${classic ? `${esc(roleLabel(data.role))} · ` : ""}${data.side === "red" ? "Rojo" : "Azul"} · nivel ${num(
    data.level
  )} · ${esc(data.modeLabel || data.mapName || data.gameMode || "")}</p>
      </div>
      <div class="client-kpis">
        <div class="client-kpi"><span>Oro</span><strong>${num(data.gold)}</strong></div>
        ${classic ? `<div class="client-kpi"><span>Minuto</span><strong>${clock(data.minute)}</strong></div>` : ""}
        <div class="client-kpi"><span>KDA</span><strong>${scores.kills || 0}/${scores.deaths || 0}/${
    scores.assists || 0
  }</strong></div>
        ${classic ? `<div class="client-kpi"><span>CS</span><strong>${num(scores.cs)}</strong></div>` : ""}
      </div>
    </div>
    <div class="client-items">${items}</div>
    <div class="client-team-grid">${allies}${enemies}</div>`;

  setBadge("#client-live-badge", "En partida", "teal");
  const button = $("#client-use-live");
  if (button) button.disabled = false;
}

function updateLiveSource() {
  const source = $("#live-source");
  if (!source) return;
  const time = new Date().toLocaleTimeString("es-ES", {
    hour: "2-digit",
    minute: "2-digit",
    second: "2-digit",
  });
  source.textContent = `Cliente local · ${time}`;
  source.classList.add("live");
}

/* ------------------------------------------------------------------ */
/* Sincronizacion                                                      */
/* ------------------------------------------------------------------ */
function draftSignature(draft) {
  return JSON.stringify({
    role: draft.role,
    side: draft.side,
    allies: draft.allies,
    enemies: draft.enemies,
    bans: draft.bans,
    my: draft.myChampion,
  });
}

async function applyDraftIfChanged() {
  if (!draftData?.available) return;
  const signature = draftSignature(draftData);
  if (signature === lastDraftSig) return;
  lastDraftSig = signature;
  applyClientDraft(draftData);
}

async function applyLiveNow() {
  if (!liveData?.available) return;
  const ok = await applyClientState(liveData);
  if (ok) updateLiveSource();
}

function maybeNotify(previous, current) {
  const before = Boolean(previous?.connected || previous?.liveClient);
  const now = Boolean(current?.connected || current?.liveClient);
  if (lastConnected === null) {
    lastConnected = now;
    return;
  }
  if (now && !lastConnected) toast("Cliente detectado. Datos en vivo disponibles.", "success");
  if (!now && lastConnected) toast("Cliente desconectado. Puedes seguir en modo manual.", "info");
  lastConnected = now;
}

async function fetchDraft() {
  try {
    const response = await api.get("/api/client/draft");
    if (response.available && response.draft?.available) {
      draftData = response.draft;
      renderDraft();
      return true;
    }
    draftData = null;
    renderDraft(response.message);
    return false;
  } catch (err) {
    draftData = null;
    renderDraft(err.message);
    return false;
  }
}

async function fetchLive() {
  try {
    const response = await api.get("/api/client/live");
    if (response.available && response.state?.available) {
      liveData = response.state;
      renderLive();
      return true;
    }
    liveData = null;
    renderLive(response.message);
    return false;
  } catch (err) {
    liveData = null;
    renderLive(err.message);
    return false;
  }
}

async function poll() {
  if (busy) return;
  if (document.hidden) return;
  busy = true;
  const previous = status;
  try {
    status = await api.get("/api/client/status");
    maybeNotify(previous, status);
    renderStatus();

    if (status.champSelect) {
      const ok = await fetchDraft();
      if (ok && auto) await applyDraftIfChanged();
      liveData = null;
      renderLive();
    } else if (status.inGame || status.liveClient) {
      const ok = await fetchLive();
      if (ok && auto) await applyLiveNow();
      renderDraft(
        draftData
          ? ""
          : "El champ select ya terminó; este panel se rellena cuando vuelvas a elegir campeón."
      );
      renderStatus();
    } else {
      draftData = null;
      liveData = null;
      renderDraft();
      renderLive();
      renderStatus();
    }
  } catch (err) {
    status = {
      ...(status || {}),
      connected: false,
      liveClient: false,
      champSelect: false,
      inGame: false,
      error: err.message,
    };
    renderStatus();
  } finally {
    busy = false;
  }
}

function ensurePolling() {
  const shouldPoll = auto || active;
  if (shouldPoll && !timer) {
    timer = setInterval(poll, 3000);
    poll();
  } else if (!shouldPoll && timer) {
    clearInterval(timer);
    timer = null;
  }
}

function syncAutoUi() {
  const checkbox = $("#client-auto");
  if (checkbox) checkbox.checked = auto;
  const liveAuto = $("#live-auto");
  if (liveAuto) {
    liveAuto.textContent = `Auto: ${auto ? "on" : "off"}`;
    liveAuto.classList.toggle("auto", auto);
  }
  const note = $("#client-auto-note");
  if (note) {
    note.textContent = auto
      ? "Sincronización activa: champ select en Draft y partida en En vivo."
      : "Cada 3 segundos mientras la aplicación esté visible. No envía inputs al juego.";
  }
}

function setAuto(value) {
  auto = Boolean(value);
  syncAutoUi();
  ensurePolling();
  if (auto) {
    lastDraftSig = null;
    toast(
      status?.connected || status?.liveClient
        ? "Sincronización automática activada."
        : "Sincronización activada; esperando al cliente de LoL.",
      "info"
    );
    poll();
  } else {
    toast("Sincronización automática desactivada.", "info");
  }
}

async function importDraft({ switchTab = false } = {}) {
  const response = await api.get("/api/client/draft").catch((err) => ({ available: false, message: err.message }));
  if (!response.available || !response.draft?.available) {
    toast(response.message || "Abre un champ select en League of Legends.", "error");
    return false;
  }
  draftData = response.draft;
  lastDraftSig = draftSignature(draftData);
  renderDraft();
  applyClientDraft(draftData);
  renderStatus();
  toast("Draft importado desde el cliente. Tu slot queda libre para comparar.", "success");
  if (switchTab) location.hash = "#draft";
  return true;
}

async function importLive({ switchTab = false } = {}) {
  const response = await api.get("/api/client/live").catch((err) => ({ available: false, message: err.message }));
  if (!response.available || !response.state?.available) {
    toast(response.message || "No hay una partida en curso.", "error");
    return false;
  }
  liveData = response.state;
  renderLive();
  const ok = await applyClientState(liveData);
  if (ok) {
    updateLiveSource();
    toast("Estado de partida importado desde el cliente.", "success");
    if (switchTab) location.hash = "#live";
  }
  renderStatus();
  return ok;
}

async function searchAgain() {
  try {
    await api.post("/api/client/search", {});
    toast("Buscando League of Legends en los discos…", "info");
    poll();
  } catch (err) {
    toast(err.message || "No se pudo iniciar la búsqueda.", "error");
  }
}

async function savePath() {
  const input = $("#client-path");
  if (!input) return;
  const value = input.value.trim();
  if (!value) {
    toast("Escribe la ruta de la carpeta de League of Legends.", "error");
    return;
  }
  try {
    const response = await api.post("/api/client/path", { path: value });
    delete input.dataset.dirty;
    if (response.search?.installPath) input.value = response.search.installPath;
    toast(`Ruta guardada: ${response.search?.installPath || value}`, "success");
    await poll();
  } catch (err) {
    toast(err.message || "Ruta no válida.", "error");
  }
}

export function setActive(value) {
  active = Boolean(value);
  ensurePolling();
  if (active) poll();
}

export function init() {
  if (initialized) return;
  initialized = true;

  $("#client-refresh")?.addEventListener("click", () => poll());
  $("#client-use-draft")?.addEventListener("click", () => importDraft({ switchTab: true }));
  $("#client-use-live")?.addEventListener("click", () => importLive({ switchTab: true }));
  $("#draft-import")?.addEventListener("click", () => importDraft());
  $("#live-sync")?.addEventListener("click", () => importLive());
  $("#client-auto")?.addEventListener("change", (event) => setAuto(event.target.checked));
  $("#live-auto")?.addEventListener("click", () => setAuto(!auto));
  $("#client-search")?.addEventListener("click", searchAgain);
  $("#client-path-save")?.addEventListener("click", savePath);
  $("#client-path")?.addEventListener("input", (event) => {
    event.target.dataset.dirty = "1";
  });
  $("#client-path")?.addEventListener("keydown", (event) => {
    if (event.key === "Enter") {
      event.preventDefault();
      savePath();
    }
  });

  document.addEventListener("visibilitychange", () => {
    if (!document.hidden && (auto || active)) poll();
  });

  syncAutoUi();
  ensurePolling();
}
