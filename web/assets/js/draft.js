/* Vista de draft: composiciones, recomendaciones y bans. */

import { api } from "./api.js";
import { meta, state, championByName, roleLabel, ROLE_SHORT } from "./state.js";
import {
  $,
  $$,
  avatarHTML,
  emptyState,
  esc,
  openChampionPicker,
  pct,
  pp,
  setModeBadge,
  skeletonList,
  toast,
} from "./ui.js";

const draft = state.draft;
let controller = null;
let requestSeq = 0;
let lastResponse = null;
let bound = false;

const DRAFT_MODES = [
  { key: "classic", label: "Grieta" },
  { key: "aram", label: "ARAM" },
  { key: "mayhem", label: "Mayhem" },
  { key: "arena", label: "Arena" },
];

function roleIdx(role) {
  return meta.roles.indexOf(role);
}

function modeLabel(mode = draft.mode) {
  return DRAFT_MODES.find((entry) => entry.key === mode)?.label || "Grieta";
}

function modeNotice(mode = draft.mode) {
  if (mode === "aram") {
    return "ARAM detectado: no hay roles ni bans. Registra los 5 aliados y los 5 rivales; la build se recomienda en En vivo.";
  }
  if (mode === "mayhem") {
    return "ARAM: Mayhem detectado: no hay roles ni bans. Los augments elegidos se corrigen en En vivo.";
  }
  if (mode === "arena") {
    return "Arena detectada: no hay draft de roles. Corrige trío, augments y anviles en En vivo.";
  }
  return "";
}

function applyModeLayout() {
  const classic = draft.mode === "classic";
  $("#draft-side-control")?.classList.toggle("hidden", !classic);
  $("#draft-role-control")?.classList.toggle("hidden", !classic);
  $("#draft-ban-card")?.classList.toggle("hidden", !classic);
  $("#draft-extra")?.classList.toggle("hidden", !classic);
  $("#ally-side-badge")?.classList.toggle("hidden", !classic);
  $("#enemy-side-badge")?.classList.toggle("hidden", !classic);

  const notice = $("#draft-mode-notice");
  if (notice) {
    notice.classList.toggle("hidden", classic);
    notice.textContent = classic ? "" : modeNotice();
  }
  const title = $("#draft-reco-title");
  if (title) title.textContent = classic ? "Mejores picks" : "Composición";
  const verdictLabel = $("#draft-verdict .verdict-label");
  if (verdictLabel) verdictLabel.textContent = classic ? "Prob. estimada" : "Composición";
  const allyTitle = $("#ally-card h2");
  const allySub = $("#ally-card .card-head p");
  const enemyTitle = $("#enemy-card h2");
  const enemySub = $("#enemy-card .card-head p");
  if (allyTitle) allyTitle.textContent = classic ? "Tu equipo" : draft.mode === "arena" ? "Tu trío" : "Tu equipo";
  if (allySub) {
    allySub.textContent = classic
      ? "Marca tu rol y deja libre tu slot"
      : draft.mode === "arena"
        ? "Tú y tus compañeros de Arena"
        : "Los cinco campeones aliados";
  }
  if (enemyTitle) enemyTitle.textContent = classic ? "Rival" : "Rivales";
  if (enemySub) {
    enemySub.textContent = classic
      ? draft.enemyRolesInferred
        ? "Roles estimados por campeón; el cliente no los muestra"
        : "Añade lo que ya viste en el draft"
      : draft.mode === "arena"
        ? "Los demás equipos de Arena"
        : "Los cinco rivales";
  }
}

function renderModes() {
  const host = $("#draft-modes");
  if (!host) return;
  host.innerHTML = DRAFT_MODES.map(
    (mode) =>
      `<button type="button" class="chip ${draft.mode === mode.key ? "active" : ""}" data-draft-mode="${mode.key}" aria-pressed="${
        draft.mode === mode.key
      }">${mode.label}</button>`
  ).join("");
}

function allPicks() {
  return [...draft.allies, ...draft.enemies].filter(Boolean);
}

function isUsed(name) {
  return [...allPicks(), ...draft.bans].some((x) => x === name);
}

/* ------------------------------------------------------------------ */
/* Render                                                              */
/* ------------------------------------------------------------------ */
function renderRoles() {
  const host = $("#role-chips");
  if (!host) return;
  host.innerHTML = meta.roles
    .map(
      (role) =>
        `<button type="button" class="chip ${draft.role === role ? "active" : ""}" data-role="${role}">${
          roleLabel(role)
        }</button>`
    )
    .join("");
}

function renderSide() {
  $$("#side-toggle button").forEach((button) => {
    button.classList.toggle("active", button.dataset.side === draft.side);
  });
  const allyBadge = $("#ally-side-badge");
  const enemyBadge = $("#enemy-side-badge");
  const blue = draft.side === "blue";
  if (allyBadge) {
    allyBadge.textContent = blue ? "Azul" : "Rojo";
    allyBadge.className = `badge ${blue ? "blue" : "red"}`;
  }
  if (enemyBadge) {
    enemyBadge.textContent = blue ? "Rojo" : "Azul";
    enemyBadge.className = `badge ${blue ? "red" : "blue"}`;
  }
}

function slotHTML(team, role, index) {
  const classic = draft.mode === "classic";
  const mine = classic ? role === draft.role : team === "allies" && index === 0;
  const name = draft[team][index];
  const ghost =
    !name && mine && classic && team === "allies" && draft.myPick && championByName(draft.myPick)
      ? draft.myPick
      : null;
  const displayed = name || ghost;
  const champ = championByName(displayed);
  const detected = Boolean(ghost);
  const estimated = classic && team === "enemies" && draft.enemyRolesInferred;
  const roleData = classic && !estimated ? champ?.roles?.[role] : null;
  const subtitle = champ
    ? detected
      ? "Detectado del cliente · slot libre para comparar"
      : `${champ.tags?.[0] || "Flex"}${roleData ? ` · ${pct(roleData.winrate, 0)} WR` : ""}`
    : mine
      ? classic
        ? "Tu pick queda libre para comparar"
        : "Tu campeón en este modo"
      : "Hueco libre";
  const roleText = classic
    ? `${ROLE_SHORT[role] || role}${estimated ? " ≈" : ""}${mine ? "<small>TU ROL</small>" : ""}`
    : `${team === "allies" ? "A" : "R"}${index + 1}${mine ? "<small>TU CAMPEÓN</small>" : ""}`;
  const pickerRole = classic ? role : "ALL";
  const title = classic
    ? champ
      ? `${detected ? "Cambiar pick detectado" : "Cambiar"} ${champ.name} en ${roleLabel(role)}`
      : `Elegir campeón para ${roleLabel(role)}`
    : champ
      ? `Cambiar ${champ.name}`
      : `Elegir ${team === "allies" ? "aliado" : "rival"} ${index + 1}`;
  return `<div class="slot-row ${mine ? "mine" : ""}">
    <div class="slot-role" ${
      estimated ? `title="Rol estimado: el cliente no expone los roles rivales"` : ""
    }>${roleText}</div>
    <div class="slot-wrap">
      <button type="button" class="slot ${champ ? "filled" : "empty"} ${mine ? "mine" : ""} ${
        detected ? "detected" : ""
      }" data-team="${team}" data-index="${index}" data-role="${pickerRole}" aria-label="${esc(title)}">
        ${
          champ
            ? `${avatarHTML(champ)}<span class="slot-text"><strong>${esc(champ.name)}</strong><small>${esc(
                subtitle
              )}</small></span>`
            : `<span class="plus"><svg class="icon"><use href="#i-plus"/></svg></span>
               <span class="slot-text"><strong>${mine ? "Elegir campeón" : "Añadir campeón"}</strong><small>${esc(
                 subtitle
               )}</small></span>`
        }
      </button>
      ${
        detected
          ? `<button type="button" class="remove-btn" data-clear-my-pick aria-label="Quitar pick detectado ${esc(
              champ.name
            )}"><svg class="icon"><use href="#i-close"/></svg></button>`
          : champ
            ? `<button type="button" class="remove-btn" data-remove="${team}" data-index="${index}" aria-label="Quitar ${esc(
                champ.name
              )}"><svg class="icon"><use href="#i-close"/></svg></button>`
            : ""
      }
    </div>
  </div>`;
}

function renderTeam(team, hostSelector) {
  const host = $(hostSelector);
  if (!host) return;
  const roles = draft.mode === "classic" ? meta.roles : Array.from({ length: 5 }, (_, index) => String(index));
  const limit = draft.mode === "arena" && team === "allies" ? 3 : roles.length;
  host.innerHTML = roles
    .slice(0, limit)
    .map((role, index) => slotHTML(team, role, index))
    .join("");
}

function renderBans() {
  const host = $("#ban-chips");
  if (!host) return;
  if (!draft.bans.length) {
    host.innerHTML = `<span class="ban-empty">Sin bans registrados. Puedes añadir hasta 10.</span>`;
    return;
  }
  host.innerHTML = draft.bans
    .map((name, index) => {
      const champ = championByName(name);
      return `<div class="ban-chip">
        ${avatarHTML(champ, "sm")}
        <span>${esc(name)}</span>
        <button type="button" class="remove" data-ban-remove="${index}" aria-label="Quitar ban de ${esc(
        name
      )}"><svg class="icon"><use href="#i-close"/></svg></button>
      </div>`;
    })
    .join("");
}

function renderAll() {
  renderModes();
  renderRoles();
  renderSide();
  renderTeam("allies", "#ally-slots");
  renderTeam("enemies", "#enemy-slots");
  renderBans();
  applyModeLayout();
}

function renderVerdict(data) {
  const value = $("#draft-verdict .verdict-value");
  const note = $("#draft-verdict .verdict-note");
  const detectedPick = draft.mode === "classic" && draft.myPick ? 1 : 0;
  const known = data
    ? (data.knownAllies || []).filter(Boolean).length +
      (data.knownEnemies || []).filter(Boolean).length +
      detectedPick
    : allPicks().length + detectedPick;
  if (draft.mode !== "classic") {
    if (value) value.textContent = "—";
    if (note) note.textContent = `${modeLabel()} · ${known}/10 campeones`;
    return;
  }
  const probability = data?.baselineWinProbability;
  if (value) value.textContent = probability == null ? "—" : pct(probability, 1);
  if (note) {
    note.textContent = data
      ? `${known}/10 picks · ${data.mode === "model" ? "modelo" : "datos pro"}`
      : "Completa el draft";
  }
}

function evidenceHTML(rec) {
  const e = rec.evidence || {};
  const modelValue = e.modelEdge != null ? pp(e.modelEdge) : "—";
  return `<div class="evidence-grid">
    <div class="evidence"><span>Modelo</span><strong>${modelValue}</strong><small>${pct(rec.winProbability, 1)} victoria</small></div>
    <div class="evidence"><span>Winrate ${esc(ROLE_SHORT[rec.role] || rec.role)}</span><strong>${pct(
    e.priorWinrate,
    1
  )}</strong><small>${e.priorGames || 0} partidas</small></div>
    <div class="evidence"><span>Matchup</span><strong>${pct(e.matchupWinrate, 1)}</strong><small>${
    e.matchupGames || 0
  } muestras</small></div>
    <div class="evidence"><span>Presencia</span><strong>${pct(e.presence, 0)}</strong><small>${pct(
    e.rolePickRate,
    1
  )} pick en el rol</small></div>
  </div>`;
}

function renderRecommendations(data) {
  const host = $("#draft-reco");
  const badge = $("#reco-mode");
  const subtitle = $("#reco-subtitle");
  if (!host) return;
  if (draft.mode !== "classic") {
    if (badge) {
      badge.textContent = modeLabel();
      badge.classList.add("priors");
    }
    if (subtitle) subtitle.textContent = "Sin picks ni roles en este modo";
    host.innerHTML = emptyState("i-info", modeLabel(), modeNotice());
    return;
  }
  setModeBadge(badge, data?.mode);

  if (data?.myChampion) {
    if (subtitle) subtitle.textContent = "Tu slot ya está fijado";
    host.innerHTML = `<div class="empty-state">
      <svg class="icon"><use href="#i-info"/></svg>
      <strong>Tu pick: ${esc(data.myChampion.name)}</strong>
      <span>Quita tu campeón del slot ${esc(roleLabel(data.role))} para comparar alternativas.</span>
      <button type="button" class="btn tiny" data-clear-mine>Quitar mi campeón</button>
    </div>`;
    return;
  }

  const recs = data?.recommendations || [];
  if (subtitle) {
    subtitle.textContent = recs.length
      ? `Top ${recs.length} para ${roleLabel(data.role)} · ${data.mode === "model" ? "modelo + datos pro" : "datos pro"}`
      : "Sin candidatos con datos";
  }
  if (!recs.length) {
    host.innerHTML = emptyState("i-info", "No hay recomendaciones", "Añade rivales o cambia el rol.");
    return;
  }

  host.innerHTML = recs
    .map((rec, index) => {
      const champ = rec.champion;
      const reasons = (rec.reasons || []).slice(0, 3);
      return `<article class="reco-item" tabindex="0" role="button" aria-expanded="false" data-reco="${index}">
        <span class="reco-rank">${index + 1}</span>
        ${avatarHTML(champ)}
        <div class="reco-main">
          <div class="reco-name"><strong>${esc(champ.name)}</strong><span class="role-tag">${
        ROLE_SHORT[rec.role] || rec.role
      }</span></div>
          <div class="reco-reasons">${reasons
            .map((r) => `<span class="reason">${esc(r)}</span>`)
            .join("")}</div>
        </div>
        <div class="reco-score">
          <strong>${Math.round(rec.score)}</strong>
          <small>índice</small>
          <div class="score-bar"><i style="width:${Math.max(3, Math.min(100, rec.score))}%"></i></div>
        </div>
        <div class="reco-detail">${evidenceHTML(rec)}</div>
      </article>`;
    })
    .join("");
}

function renderBanPriority(items) {
  const host = $("#ban-priority");
  if (!host) return;
  if (!items?.length) {
    host.innerHTML = emptyState("i-info", "Sin datos de bans");
    return;
  }
  host.innerHTML = items
    .map((item) => {
      const champ = championByName(item.name);
      return `<button type="button" class="ban-priority-item" data-ban="${esc(item.name)}" ${
        isUsed(item.name) ? "disabled" : ""
      }>
        ${avatarHTML(champ)}
        <span class="bp-main">
          <strong>${esc(item.name)}</strong>
          <small>${pct(item.presence, 0)} presencia · ${pct(item.winrate, 1)} WR</small>
        </span>
        <span class="bp-score">${Math.round(item.priority)}</span>
      </button>`;
    })
    .join("");
}

function renderResponse(data) {
  lastResponse = data;
  renderVerdict(data);
  renderRecommendations(data);
  renderBanPriority(data.banPriority);
}

function setBusy() {
  const host = $("#draft-reco");
  if (host) skeletonList(host, 6);
  const badge = $("#reco-mode");
  if (badge) {
    badge.textContent = "Calculando";
    badge.classList.add("priors");
  }
  const subtitle = $("#reco-subtitle");
  if (subtitle) subtitle.textContent = "Consultando el modelo y el dataset…";
  renderVerdict(null);
}

function renderError(err) {
  const host = $("#draft-reco");
  if (host) {
    host.innerHTML = `<div class="empty-state">
      <svg class="icon"><use href="#i-info"/></svg>
      <strong>No se pudo calcular</strong>
      <span>${esc(err.message || "Error desconocido")}</span>
    </div>`;
  }
  toast(err.message || "No se pudo calcular el draft", "error");
}

/* ------------------------------------------------------------------ */
/* Datos                                                               */
/* ------------------------------------------------------------------ */
function payload() {
  return {
    role: draft.role,
    side: draft.side,
    bans: draft.bans,
    allies: draft.allies,
    enemies: draft.enemies,
    myChampion: draft.myPick || null,
    limit: 12,
  };
}

async function refresh() {
  if (!state.meta) return;
  if (draft.mode !== "classic") {
    controller?.abort();
    requestSeq += 1;
    renderVerdict(null);
    renderRecommendations(null);
    renderBanPriority([]);
    applyModeLayout();
    return;
  }
  const seq = ++requestSeq;
  controller?.abort();
  controller = new AbortController();
  setBusy();
  try {
    const data = await api.post("/api/draft", payload(), { signal: controller.signal });
    if (seq !== requestSeq) return;
    renderResponse(data);
  } catch (err) {
    if (err.name === "AbortError" || seq !== requestSeq) return;
    renderError(err);
  }
}

async function chooseChampion(team, role, index) {
  const slotIndex = Number(index);
  if (!Number.isInteger(slotIndex) || slotIndex < 0 || slotIndex >= 5) return;
  const current = draft[team][slotIndex];
  const exclude = [...allPicks().filter((name) => name !== current), ...draft.bans];
  const title =
    draft.mode === "classic"
      ? team === "allies"
        ? `Tu equipo · ${roleLabel(role)}`
        : `Rival · ${roleLabel(role)}`
      : `${team === "allies" ? "Aliado" : "Rival"} ${slotIndex + 1}`;
  const pickerRole = draft.mode === "classic" ? role : "ALL";
  const name = await openChampionPicker({
    role: pickerRole,
    exclude,
    title,
    kicker: draft.mode === "classic" ? "Elegir campeón" : modeLabel(),
  });
  if (!name) return;
  if (team === "allies" && draft.mode === "classic" && slotIndex === roleIdx(draft.role)) {
    draft.myPick = null;
  }
  draft[team][slotIndex] = name;
  renderAll();
  refresh();
}

async function addBan() {
  if (draft.bans.length >= 10) {
    toast("Ya hay 10 bans registrados.", "error");
    return;
  }
  const exclude = [...allPicks(), ...draft.bans];
  const name = await openChampionPicker({
    role: "ALL",
    exclude,
    title: "Añadir ban",
    kicker: "Bans de la partida",
  });
  if (!name) return;
  addBanByName(name);
}

function addBanByName(name) {
  if (!name) return;
  if (draft.bans.length >= 10) {
    toast("Ya hay 10 bans registrados.", "error");
    return;
  }
  if (isUsed(name)) {
    toast(`${name} ya está en el draft.`, "error");
    return;
  }
  draft.bans.push(name);
  renderAll();
  refresh();
}

function removeBan(index) {
  draft.bans.splice(index, 1);
  renderAll();
  refresh();
}

function setDraftMode(mode) {
  if (!DRAFT_MODES.some((entry) => entry.key === mode) || mode === draft.mode) return;
  draft.mode = mode;
  draft.myPick = null;
  draft.enemyRolesInferred = false;
  if (mode !== "classic") draft.bans = [];
  renderAll();
  refresh();
  toast(`Modo: ${modeLabel(mode)}.`, "info");
}

function loadExample() {
  const pad = (list) =>
    Array.from({ length: 5 }, (_, index) => {
      const name = list[index];
      return name && championByName(name) ? name : null;
    });
  draft.myPick = null;
  draft.enemyRolesInferred = false;
  if (draft.mode === "classic") {
    draft.role = "MID";
    draft.side = "blue";
    draft.allies = pad(["K'Sante", "Lee Sin", null, "Jinx", "Rakan"]);
    draft.enemies = pad(["Gnar", "Vi", "Orianna", "Kai'Sa", "Nautilus"]);
    draft.bans = ["Yone", "Aurora", "Rell", "Ambessa", "Poppy"]
      .filter((name) => championByName(name))
      .slice(0, 10);
    toast("Draft de ejemplo cargado. Tu slot MID queda libre.", "success");
  } else {
    draft.allies =
      draft.mode === "arena"
        ? pad(["Ahri", "Rakan", "Gnar"])
        : pad(["Ahri", "Garen", "Jinx", "Rakan", "Lee Sin"]);
    draft.enemies = pad(["Gnar", "Vi", "Orianna", "Kai'Sa", "Nautilus"]);
    draft.bans = [];
    toast(`Composición de ejemplo cargada: ${modeLabel()}.`, "success");
  }
  renderAll();
  refresh();
}

function clearDraft() {
  draft.allies = [null, null, null, null, null];
  draft.enemies = [null, null, null, null, null];
  draft.bans = [];
  draft.myPick = null;
  draft.enemyRolesInferred = false;
  renderAll();
  refresh();
}

/* ------------------------------------------------------------------ */
/* Eventos                                                             */
/* ------------------------------------------------------------------ */
export function init() {
  if (bound) return;
  bound = true;
  renderAll();
  refresh();

  $("#draft-modes")?.addEventListener("click", (event) => {
    const button = event.target.closest("[data-draft-mode]");
    if (button) setDraftMode(button.dataset.draftMode);
  });

  $("#role-chips")?.addEventListener("click", (event) => {
    const button = event.target.closest("button[data-role]");
    if (!button || button.dataset.role === draft.role) return;
    draft.role = button.dataset.role;
    draft.myPick = null;
    renderAll();
    refresh();
  });

  $("#side-toggle")?.addEventListener("click", (event) => {
    const button = event.target.closest("button[data-side]");
    if (!button || button.dataset.side === draft.side) return;
    draft.side = button.dataset.side;
    renderAll();
    refresh();
  });

  for (const [team, selector] of [
    ["allies", "#ally-slots"],
    ["enemies", "#enemy-slots"],
  ]) {
    $(selector)?.addEventListener("click", (event) => {
      const clearMine = event.target.closest("[data-clear-my-pick]");
      if (clearMine) {
        draft.myPick = null;
        renderAll();
        refresh();
        return;
      }
      const remove = event.target.closest("[data-remove]");
      if (remove) {
        const index = Number(remove.dataset.index);
        draft[remove.dataset.remove][index] = null;
        renderAll();
        refresh();
        return;
      }
      const slot = event.target.closest("[data-team]");
      if (slot) chooseChampion(slot.dataset.team, slot.dataset.role, slot.dataset.index);
    });
  }

  $("#ban-chips")?.addEventListener("click", (event) => {
    const button = event.target.closest("[data-ban-remove]");
    if (!button) return;
    removeBan(Number(button.dataset.banRemove));
  });

  $("#add-ban")?.addEventListener("click", addBan);

  $("#ban-priority")?.addEventListener("click", (event) => {
    const button = event.target.closest("[data-ban]");
    if (!button || button.disabled) return;
    addBanByName(button.dataset.ban);
  });

  $("#draft-example")?.addEventListener("click", loadExample);
  $("#draft-clear")?.addEventListener("click", clearDraft);

  $("#draft-reco")?.addEventListener("click", (event) => {
    if (event.target.closest("[data-clear-mine]")) {
      const index = roleIdx(draft.role);
      if (index >= 0) draft.allies[index] = null;
      draft.myPick = null;
      renderAll();
      refresh();
      return;
    }
    const item = event.target.closest(".reco-item");
    if (!item) return;
    const open = item.classList.toggle("open");
    item.setAttribute("aria-expanded", open ? "true" : "false");
  });

  $("#draft-reco")?.addEventListener("keydown", (event) => {
    if (event.key !== "Enter" && event.key !== " ") return;
    const item = event.target.closest(".reco-item");
    if (!item) return;
    event.preventDefault();
    item.click();
  });
}

/* ------------------------------------------------------------------ */
/* Integracion con el cliente local                                    */
/* ------------------------------------------------------------------ */
export function applyClientDraft(mapped) {
  if (!mapped?.available) return false;
  const role = meta.roles.includes(mapped.role) ? mapped.role : draft.role;
  const detected = DRAFT_MODES.some((entry) => entry.key === mapped.gameMode)
    ? mapped.gameMode
    : draft.mode;
  const changed = detected !== draft.mode;
  const pad = (list) =>
    Array.from({ length: 5 }, (_, index) => {
      const name = Array.isArray(list) ? list[index] : null;
      return name && championByName(name) ? name : null;
    });

  draft.mode = detected;
  draft.role = role;
  draft.side = mapped.side === "red" ? "red" : "blue";
  draft.allies = pad(mapped.allies);
  draft.enemies = pad(mapped.enemies);
  draft.enemyRolesInferred =
    detected === "classic" && mapped.enemyRolesInferred === true;
  const roleIndex = roleIdx(role);
  draft.myPick =
    mapped.myChampion && championByName(mapped.myChampion)
      ? mapped.myChampion
      : roleIndex >= 0 && draft.allies[roleIndex] && championByName(draft.allies[roleIndex])
        ? draft.allies[roleIndex]
        : null;
  draft.bans =
    detected === "classic" && Array.isArray(mapped.bans)
      ? mapped.bans.filter((name) => championByName(name)).slice(0, 10)
      : [];

  // En Grieta el slot propio se deja libre para comparar alternativas; el
  // campeón detectado se muestra como pick fantasma en ese hueco. En el resto
  // de modos el mapper ya coloca tu campeón en el primer hueco.
  if (detected === "classic" && roleIndex >= 0) {
    if (draft.myPick) {
      draft.allies = draft.allies.map((name, index) =>
        index !== roleIndex && name === draft.myPick ? null : name
      );
    }
    draft.allies[roleIndex] = null;
  }

  renderAll();
  refresh();
  if (changed) toast(`Modo detectado: ${modeLabel(detected)}.`, "info");
  return true;
}
