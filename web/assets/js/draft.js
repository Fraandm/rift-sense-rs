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

function roleIdx(role) {
  return meta.roles.indexOf(role);
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
  const mine = role === draft.role;
  const name = draft[team][index];
  const champ = championByName(name);
  const roleData = champ?.roles?.[role];
  const subtitle = champ
    ? `${champ.tags?.[0] || "Flex"}${roleData ? ` · ${pct(roleData.winrate, 0)} WR` : ""}`
    : mine
    ? "Tu pick queda libre para comparar"
    : "Hueco libre";
  return `<div class="slot-row ${mine ? "mine" : ""}">
    <div class="slot-role">${ROLE_SHORT[role] || role}${mine ? "<small>TU ROL</small>" : ""}</div>
    <div class="slot-wrap">
      <button type="button" class="slot ${champ ? "filled" : "empty"} ${mine ? "mine" : ""}" data-team="${team}" data-role="${role}" aria-label="${
        champ ? `Cambiar ${champ.name} en ${roleLabel(role)}` : `Elegir campeón para ${roleLabel(role)}`
      }">
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
        champ
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
  host.innerHTML = meta.roles.map((role, index) => slotHTML(team, role, index)).join("");
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
  renderRoles();
  renderSide();
  renderTeam("allies", "#ally-slots");
  renderTeam("enemies", "#enemy-slots");
  renderBans();
}

function renderVerdict(data) {
  const value = $("#draft-verdict .verdict-value");
  const note = $("#draft-verdict .verdict-note");
  const probability = data?.baselineWinProbability;
  if (value) value.textContent = probability == null ? "—" : pct(probability, 1);
  const known = data ? (data.knownAllies || []).filter(Boolean).length + (data.knownEnemies || []).filter(Boolean).length : 0;
  if (note) {
    note.textContent = data ? `${known}/10 picks · ${data.mode === "model" ? "modelo" : "datos pro"}` : "Completa el draft";
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
    limit: 12,
  };
}

async function refresh() {
  if (!state.meta) return;
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

async function chooseChampion(team, role) {
  const index = roleIdx(role);
  if (index < 0) return;
  const current = draft[team][index];
  const exclude = [...allPicks().filter((name) => name !== current), ...draft.bans];
  const title = team === "allies" ? `Tu equipo · ${roleLabel(role)}` : `Rival · ${roleLabel(role)}`;
  const name = await openChampionPicker({ role, exclude, title, kicker: "Elegir campeón" });
  if (!name) return;
  draft[team][index] = name;
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

function loadExample() {
  const allies = ["K'Sante", "Lee Sin", null, "Jinx", "Rakan"];
  const enemies = ["Gnar", "Vi", "Orianna", "Kai'Sa", "Nautilus"];
  const bans = ["Yone", "Aurora", "Rell", "Ambessa", "Poppy"];
  draft.role = "MID";
  draft.side = "blue";
  draft.allies = allies.map((n) => (n && championByName(n) ? n : null));
  draft.enemies = enemies.map((n) => (n && championByName(n) ? n : null));
  draft.bans = bans.filter((n) => championByName(n)).slice(0, 10);
  renderAll();
  refresh();
  toast("Draft de ejemplo cargado. Tu slot MID queda libre.", "success");
}

function clearDraft() {
  draft.allies = [null, null, null, null, null];
  draft.enemies = [null, null, null, null, null];
  draft.bans = [];
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

  $("#role-chips")?.addEventListener("click", (event) => {
    const button = event.target.closest("button[data-role]");
    if (!button || button.dataset.role === draft.role) return;
    draft.role = button.dataset.role;
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
      const remove = event.target.closest("[data-remove]");
      if (remove) {
        const index = Number(remove.dataset.index);
        draft[remove.dataset.remove][index] = null;
        renderAll();
        refresh();
        return;
      }
      const slot = event.target.closest("[data-team][data-role]");
      if (slot) chooseChampion(slot.dataset.team, slot.dataset.role);
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
  const pad = (list) =>
    Array.from({ length: 5 }, (_, index) => {
      const name = Array.isArray(list) ? list[index] : null;
      return name && championByName(name) ? name : null;
    });

  draft.role = role;
  draft.side = mapped.side === "red" ? "red" : "blue";
  draft.allies = pad(mapped.allies);
  draft.enemies = pad(mapped.enemies);
  draft.bans = Array.isArray(mapped.bans)
    ? mapped.bans.filter((name) => championByName(name)).slice(0, 10)
    : [];

  // El slot propio se deja libre para comparar alternativas, incluso si el
  // jugador ya tiene un hover en el cliente.
  const index = roleIdx(role);
  if (index >= 0) draft.allies[index] = null;

  renderAll();
  refresh();
  return true;
}
