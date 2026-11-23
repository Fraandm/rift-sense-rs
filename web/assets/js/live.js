/* Vista en vivo: estado de partida, inventario y recomendaciones de compra. */

import { api } from "./api.js";
import {
  meta,
  state,
  championByName,
  iconUrl,
  itemById,
  itemByName,
  roleIndex,
  roleLabel,
} from "./state.js";
import { recordSession } from "./session.js";
import {
  $,
  $$,
  avatarHTML,
  emptyState,
  esc,
  num,
  openChampionPicker,
  openItemPicker,
  pct,
  renderGauge,
  setModeBadge,
  skeletonList,
  toast,
} from "./ui.js";

const live = state.live;
let controller = null;
let requestSeq = 0;
let bound = false;
let lastData = null;
let itemsExpanded = false;
let lastAnnouncedPurchase = "";
let lastItemsSignature = "";
let lastMobileBuySignature = "";

const MODES = [
  { key: "classic", label: "Grieta" },
  { key: "aram", label: "ARAM" },
  { key: "mayhem", label: "Mayhem" },
  { key: "arena", label: "Arena" },
];

const MODE_INFO = {
  classic: {
    title: "Draft favorable",
    empty: "Elige un campeón y añade rivales para recibir recomendaciones de build con contexto.",
    champion: "La estimación del draft usa los picks y el modelo de Grieta. Ajusta oro e inventario para la siguiente compra.",
  },
  aram: {
    title: "ARAM",
    empty: "Elige un campeón y añade rivales. RiftSense adapta la compra a ARAM: sin recall, oro pasivo y sustain.",
    champion: "ARAM: sin recall, oro pasivo y compras al morir. La build prioriza sustain, teamfight y objetos legales en el mapa.",
  },
  mayhem: {
    title: "ARAM: Mayhem",
    empty: "Elige un campeón. RiftSense filtra objetos legales en Mayhem y añade los exclusivos del modo.",
    champion: "ARAM: Mayhem: runas desactivadas y augments. Se filtran objetos no disponibles y se añaden los exclusivos del modo.",
  },
  arena: {
    title: "Arena",
    empty: "Elige un campeón y tus rivales. En Arena se recomiendan objetos completos y se ignoran CS y roles.",
    champion: "Arena: compra objetos completos entre rondas. Sin CS ni roles; los anvils y augments se eligen en la tienda.",
  },
};

function normalizeMode(value) {
  const text = String(value || "").trim().toLowerCase();
  if (text === "kiwi" || text === "mayhem") return "mayhem";
  if (text === "cherry" || text === "arena") return "arena";
  if (text === "aram" || text === "map12" || text === "map35" || text === "map36") return "aram";
  if (text === "classic" || text === "map11" || text === "sr") return "classic";
  return null;
}

function modeLabel(mode = live.mode) {
  return MODES.find((entry) => entry.key === mode)?.label || "Grieta";
}

function ownIndex() {
  return live.mode === "classic" ? roleIndex(live.role) : 0;
}

const ARENA_STARTER_IDS = new Set([2051, 3112, 3177, 3184]);
const SUPPORT_ITEM_IDS = new Set([3865, 3866, 3867, 3869, 3870, 3871, 3876, 3877]);
const SUPPORT_FINAL_IDS = new Set([3869, 3870, 3871, 3876, 3877]);
const MAX_AUGMENTS = 4;
const ANVIL_LABELS = {
  stats: "Stats",
  legendary: "Legendario",
  prismatic: "Prismático",
};
const TIER3_BOOTS = new Map([
  [3006, 3172],
  [3008, 3168],
  [3009, 3170],
  [3020, 3175],
  [3047, 3174],
  [3111, 3173],
  [3158, 3171],
]);

function isArenaStarter(id) {
  return ARENA_STARTER_IDS.has(Number(id));
}

function slotCapFor(mode, role, minute, ownedLength) {
  return mode === "classic" &&
    role === "BOT" &&
    (Number(minute) >= 16 || Number(ownedLength) >= 7)
    ? 7
    : 6;
}

function maxSlots() {
  return slotCapFor(live.mode, live.role, live.minute, live.owned.length);
}

function botQuestDone() {
  return live.mode === "classic" && live.role === "BOT" && Number(live.minute) >= 16;
}

function regularItemCount() {
  return live.owned.filter((ownedId) => !itemById(ownedId)?.boots).length;
}

function bootsUpgrade(id) {
  return TIER3_BOOTS.get(Number(id)) || null;
}

function currentBoots() {
  return live.owned
    .map((ownedId) => itemById(ownedId))
    .find((item) => item && item.boots);
}

function isMidBootsUpgrade(id) {
  const boots = currentBoots();
  return Boolean(
    boots &&
      live.mode === "classic" &&
      live.role === "MID" &&
      Number(live.minute) >= 16 &&
      bootsUpgrade(boots.id) === Number(id)
  );
}

function currentSupportItem() {
  return live.owned
    .map((ownedId) => itemById(ownedId))
    .find((item) => item && SUPPORT_ITEM_IDS.has(Number(item.id)));
}

function supportStepAllowed(id) {
  const numeric = Number(id);
  if (!SUPPORT_ITEM_IDS.has(numeric)) return true;
  if (live.mode !== "classic" || live.role !== "SUPPORT") return false;
  const current = currentSupportItem();
  if (!current) return numeric === 3865;
  switch (Number(current.id)) {
    case 3865:
      return numeric === 3866;
    case 3866:
      return numeric === 3867;
    case 3867:
      return SUPPORT_FINAL_IDS.has(numeric);
    default:
      return false;
  }
}

function confidenceFor(build) {
  if (!live.champion) {
    return { score: 0.1, label: "Baja", tone: "bad", reasons: ["Sin campeón seleccionado"] };
  }
  if (build?.mode === "complete" && !build?.nextLegendary) {
    return {
      score: 0.92,
      label: "Alta",
      tone: "teal",
      reasons: ["Build completa: no hay siguiente objeto que recomendar"],
    };
  }

  let score = 0.38;
  const reasons = [];
  const hasModel =
    build?.mode === "model" || build?.recommendationMode === "model_first";
  if (hasModel) {
    score += 0.16;
    reasons.push("Modelo BuildNet disponible");
  } else {
    reasons.push("Sin modelo ONNX: se usan priors");
  }

  const known = Number(build?.enemyProfile?.known) || 0;
  if (known >= 5) {
    score += 0.1;
    reasons.push("5 rivales identificados");
  } else if (known > 0) {
    score += 0.05;
    reasons.push(`Solo ${known} rivales identificados`);
  } else {
    reasons.push("Faltan rivales: perfil de daño aproximado");
  }

  if (build?.gameAnalysis) {
    score += 0.09;
    reasons.push("Contexto de partida en vivo");
  } else {
    reasons.push("Sin contexto de partida en vivo");
  }

  const candidates = build?.items || [];
  const prior = Math.max(
    ...candidates.slice(0, 3).map((item) => Number(item.priorGames) || 0),
    Number(build?.nextPurchase?.priorGames) || 0
  );
  if (prior >= 50) {
    score += 0.09;
    reasons.push("Muestra amplia para los objetos principales");
  } else if (prior >= 10) {
    score += 0.05;
    reasons.push("Muestra moderada para los objetos principales");
  } else {
    reasons.push("Muestra pequeña para los objetos principales");
  }

  const top = candidates[0] || build?.nextPurchase || null;
  if (top?.replacement) {
    score += 0.06;
    reasons.push("Mejora gratuita de role quest");
  }

  const topScore = Number(candidates[0]?.score) || 0;
  const secondScore = Number(candidates[1]?.score) || 0;
  const margin = topScore - secondScore;
  if (margin >= 10) {
    score += 0.04;
    reasons.push("Recomendación clara frente a la alternativa");
  } else if (margin >= 5) {
    score += 0.02;
    reasons.push("Recomendación moderadamente clara");
  } else if (candidates.length > 1) {
    reasons.push("Alternativas muy igualadas");
  }

  const modeKey = String(build?.gameMode || live.mode || "classic").toLowerCase();
  if (modeKey !== "classic") {
    if (modeKey === "aram") {
      score -= 0.03;
      reasons.push("ARAM: objetos legales con datos de Grieta adaptados");
    } else {
      score -= 0.05;
      reasons.push("Modo con datos específicos limitados");
    }
  }

  score = Math.max(0.15, Math.min(0.92, score));
  const label = score >= 0.72 ? "Alta" : score >= 0.5 ? "Media" : "Baja";
  const tone = score >= 0.72 ? "teal" : score >= 0.5 ? "gold" : "bad";
  return { score, label, tone, reasons };
}

/* ------------------------------------------------------------------ */
/* Estado                                                              */
/* ------------------------------------------------------------------ */
function syncChampionSlot() {
  const idx = ownIndex();
  live.allies = live.allies.map((name) => (name === live.champion ? null : name));
  if (live.champion) live.allies[idx] = live.champion;
}

function setChampion(name) {
  if (!name) return;
  const idx = ownIndex();
  live.allies = live.allies.map((n) => (n === name ? null : n));
  live.champion = name;
  live.allies[idx] = name;
  live.targetLegendary = null;
  itemsExpanded = false;
  lastAnnouncedPurchase = "";
  lastItemsSignature = "";
  lastMobileBuySignature = "";
  renderAll();
  refresh(true);
}

function changeRole(role) {
  if (live.mode !== "classic" || role === live.role) return;
  const oldIdx = roleIndex(live.role);
  const newIdx = roleIndex(role);
  const oldChamp = live.champion;
  if (oldChamp && live.allies[oldIdx] === oldChamp) live.allies[oldIdx] = null;
  live.role = role;
  live.roleQuestBoots = null;
  live.targetLegendary = null;
  itemsExpanded = false;
  lastAnnouncedPurchase = "";
  lastItemsSignature = "";
  lastMobileBuySignature = "";
  if (oldChamp) live.allies[newIdx] = oldChamp;
  renderAll();
  refresh(true);
}

function setMode(mode) {
  if (!MODES.some((entry) => entry.key === mode) || mode === live.mode) return;
  const oldIdx = ownIndex();
  const champion = live.champion;
  if (champion && live.allies[oldIdx] === champion) live.allies[oldIdx] = null;
  live.mode = mode;
  live.modeLabel = modeLabel(mode);
  live.gameId = null;
  live.targetLegendary = null;
  live.enemyRolesInferred = false;
  live.roleQuestBoots = null;
  live.augments = [];
  live.anvils = [];
  itemsExpanded = false;
  lastAnnouncedPurchase = "";
  lastItemsSignature = "";
  lastMobileBuySignature = "";
  if (champion) live.allies[ownIndex()] = champion;
  applyModeLayout();
  renderAll();
  refresh(true);
}

function addItem(id) {
  const numeric = Number(id);
  const item = itemById(numeric);
  if (live.owned.includes(numeric)) return;
  if (!supportStepAllowed(numeric)) {
    toast("Ese objeto de soporte no es la siguiente mejora de la cadena.", "error");
    return;
  }
  if (isMidBootsUpgrade(numeric)) {
    const boots = currentBoots();
    live.owned = live.owned.map((ownedId) =>
      Number(ownedId) === Number(boots.id) ? numeric : ownedId
    );
    renderInventory();
    refresh(true);
    return;
  }
  const supportItem = currentSupportItem();
  if (SUPPORT_ITEM_IDS.has(numeric) && supportItem) {
    live.owned = live.owned.map((ownedId) =>
      Number(ownedId) === Number(supportItem.id) ? numeric : ownedId
    );
    renderInventory();
    refresh(true);
    return;
  }
  const cap = maxSlots();
  if (live.owned.length >= cap) {
    toast(`El inventario ya tiene ${cap} objetos.`, "error");
    return;
  }
  if (cap === 7 && botQuestDone() && !item?.boots && regularItemCount() >= 6) {
    toast("La septima ranura esta reservada para las botas de role quest.", "error");
    return;
  }
  if (item?.boots && currentBoots()) {
    toast("Ya llevas unas botas. Vende las actuales antes de comprar otras.", "error");
    return;
  }
  live.owned.push(numeric);
  if (item?.boots) live.roleQuestBoots = numeric;
  renderInventory();
  refresh(true);
}

function removeItem(index) {
  const removed = live.owned[index];
  live.owned.splice(index, 1);
  if (itemById(removed)?.boots) live.roleQuestBoots = null;
  renderInventory();
  refresh(true);
}

function buyItem(id, price = null, name = "", consumes = []) {
  let item = itemById(id);
  if (!item && name) {
    item = { id, name, icon: `${id}.png`, gold: Number(price) || 0, tags: [] };
    meta.itemsById.set(id, item);
  }
  if (!item) return;
  const numeric = Number(id);
  if (!supportStepAllowed(numeric)) {
    toast("Ese objeto de soporte no es la siguiente mejora de la cadena.", "error");
    return;
  }
  if (isMidBootsUpgrade(numeric)) {
    const boots = currentBoots();
    live.owned = live.owned.map((ownedId) =>
      Number(ownedId) === Number(boots.id) ? numeric : ownedId
    );
    renderInventory();
    refresh(true);
    return;
  }
  const supportItem = currentSupportItem();
  if (SUPPORT_ITEM_IDS.has(numeric) && supportItem) {
    live.owned = live.owned.map((ownedId) =>
      Number(ownedId) === Number(supportItem.id) ? numeric : ownedId
    );
    renderInventory();
    refresh(true);
    return;
  }
  const cost = Number(price ?? item.gold) || 0;
  const cap = maxSlots();
  const nextOwned = [...live.owned];
  for (const consumeId of Array.isArray(consumes) ? consumes.map(Number) : []) {
    const index = nextOwned.indexOf(consumeId);
    if (index < 0) {
      toast("Tu inventario cambió; vuelve a leer la recomendación.", "info");
      refresh(true);
      return;
    }
    nextOwned.splice(index, 1);
  }
  if (nextOwned.length >= cap) {
    toast("Inventario completo.", "error");
    return;
  }
  const nextRegular = nextOwned.filter((ownedId) => !itemById(ownedId)?.boots).length;
  if (cap === 7 && botQuestDone() && !item.boots && nextRegular >= 6) {
    toast("La septima ranura esta reservada para las botas de role quest.", "error");
    return;
  }
  if (nextOwned.includes(numeric)) return;
  if (item.boots && currentBoots()) {
    toast("Ya llevas unas botas. Vende las actuales antes de comprar otras.", "error");
    return;
  }
  if (live.gold < cost) {
    toast(`Te faltan ${num(cost - live.gold)} de oro para ${item.name}.`, "error");
    return;
  }
  live.gold = Math.max(0, live.gold - cost);
  live.owned = nextOwned;
  live.owned.push(numeric);
  if (item.boots) live.roleQuestBoots = numeric;
  const input = $("#live-gold");
  if (input) input.value = String(live.gold);
  renderInventory();
  refresh(true);
}

/* ------------------------------------------------------------------ */
/* Render                                                              */
/* ------------------------------------------------------------------ */
function applyModeLayout() {
  const classic = live.mode === "classic";
  const manual = live.mode === "arena" || live.mode === "mayhem";
  $("#live-role-field")?.classList.toggle("hidden", !classic);
  $("#live-side-field")?.classList.toggle("hidden", !classic);
  $("#live-minute-field")?.classList.toggle("hidden", live.mode === "arena");
  $("#live-augments-field")?.classList.toggle("hidden", !manual);
  $("#live-anvils-field")?.classList.toggle("hidden", live.mode !== "arena");
  $("#live-manual-note")?.classList.toggle("hidden", !manual);
  document.querySelector(".live-hero")?.classList.toggle("no-gauge", !classic);
  const allyLabel = $("#live-allies-label");
  if (allyLabel) allyLabel.textContent = live.mode === "arena" ? "Tu trío" : "Aliados";
  document.querySelector("#view-live")?.classList.toggle("mode-arena", live.mode === "arena");
  document.querySelector("#view-live")?.classList.toggle("mode-aram", live.mode !== "classic" && live.mode !== "arena");
}

function renderManual() {
  const augmentCount = $("#live-augment-count");
  if (augmentCount) augmentCount.textContent = `${live.augments.length}/${MAX_AUGMENTS}`;
  const augmentHost = $("#live-augment-chips");
  if (augmentHost) {
    augmentHost.innerHTML = live.augments.length
      ? live.augments
          .map(
            (name, index) =>
              `<span class="manual-chip">${esc(name)}<button type="button" data-augment-remove="${index}" aria-label="Quitar ${esc(
                name
              )}"><svg class="icon" style="width:9px;height:9px"><use href="#i-close"/></svg></button></span>`
          )
          .join("")
      : `<span class="manual-empty">Sin augments registrados.</span>`;
  }
  const anvilHost = $("#live-anvil-chips");
  if (anvilHost) {
    anvilHost.innerHTML = live.anvils.length
      ? live.anvils
          .map(
            (type, index) =>
              `<span class="manual-chip">${esc(ANVIL_LABELS[type] || type)}<button type="button" data-anvil-remove="${index}" aria-label="Quitar anvil ${esc(
                ANVIL_LABELS[type] || type
              )}"><svg class="icon" style="width:9px;height:9px"><use href="#i-close"/></svg></button></span>`
          )
          .join("")
      : `<span class="manual-empty">Sin anviles registrados.</span>`;
  }
}

function addAugment() {
  const input = $("#live-augment-input");
  const value = String(input?.value || "").trim();
  if (!value) return;
  if (live.augments.length >= MAX_AUGMENTS) {
    toast(`Puedes registrar hasta ${MAX_AUGMENTS} augments.`, "error");
    return;
  }
  if (live.augments.some((name) => name.toLowerCase() === value.toLowerCase())) {
    toast("Ese augment ya está registrado.", "info");
    return;
  }
  live.augments.push(value);
  if (input) input.value = "";
  renderManual();
  if (lastData) renderNotes(lastData.build, lastData.draft);
  refresh();
}

function removeAugment(index) {
  live.augments.splice(index, 1);
  renderManual();
  if (lastData) renderNotes(lastData.build, lastData.draft);
  refresh();
}

function addAnvil(type) {
  if (!ANVIL_LABELS[type]) return;
  live.anvils.push(type);
  renderManual();
  if (lastData) renderNotes(lastData.build, lastData.draft);
  refresh();
}

function removeAnvil(index) {
  live.anvils.splice(index, 1);
  renderManual();
  if (lastData) renderNotes(lastData.build, lastData.draft);
  refresh();
}

function renderModes() {
  const host = $("#live-modes");
  if (!host) return;
  host.innerHTML = MODES.map(
    (mode) =>
      `<button type="button" class="chip ${live.mode === mode.key ? "active" : ""}" data-live-mode="${mode.key}">${mode.label}</button>`
  ).join("");
}

function renderRoles() {
  const host = $("#live-roles");
  if (!host) return;
  host.innerHTML = meta.roles
    .map(
      (role) =>
        `<button type="button" class="chip ${live.role === role ? "active" : ""}" data-live-role="${role}">${
          roleLabel(role)
        }</button>`
    )
    .join("");
}

function renderSide() {
  $$("#live-side button").forEach((button) => {
    button.classList.toggle("active", button.dataset.side === live.side);
  });
}

function renderChampionSelect() {
  const champ = championByName(live.champion);
  const avatar = $("#live-champ-avatar");
  const name = $("#live-champ-name");
  if (avatar) avatar.innerHTML = champ ? avatarHTML(champ) : "";
  if (name) name.textContent = champ ? champ.name : "Elegir campeón";
  const kicker = $("#live-champ-role");
  if (kicker) {
    kicker.textContent = champ
      ? `${champ.name} · ${live.mode === "classic" ? roleLabel(live.role) : modeLabel()}`
      : `Sin campeón · ${modeLabel()}`;
  }
}

function renderMiniSlots() {
  const classic = live.mode === "classic";
  for (const [team, selector] of [
    ["allies", "#live-allies"],
    ["enemies", "#live-enemies"],
  ]) {
    const host = $(selector);
    if (!host) continue;
    const limit = live.mode === "arena" && team === "allies" ? 3 : 5;
    host.innerHTML = meta.roles
      .slice(0, limit)
      .map((role, index) => {
        const name = live[team][index];
        const champ = championByName(name);
        const mine = team === "allies" && index === ownIndex();
        const estimated = classic && team === "enemies" && live.enemyRolesInferred;
        const slotLabel = classic
          ? `${roleLabel(role).slice(0, 3).toUpperCase()}${estimated ? " ≈" : ""}`
          : `${team === "allies" ? "A" : "R"}${index + 1}`;
        const title = champ
          ? `Cambiar ${champ.name}`
          : classic
            ? `Elegir ${roleLabel(role)}${estimated ? " (rol estimado)" : ""}`
            : `${team === "allies" ? "Aliado" : "Rival"} ${index + 1}`;
        return `<button type="button" class="mini-slot ${champ ? "filled" : ""} ${mine ? "mine" : ""}" data-mini-team="${team}" data-index="${index}" title="${esc(title)}" aria-label="${esc(title)}">
          ${champ ? avatarHTML(champ) : `<svg class="icon"><use href="#i-plus"/></svg>`}
          <span class="mini-role">${slotLabel}</span>
        </button>`;
      })
      .join("");
  }
  const allyLimit = live.mode === "arena" ? 3 : 5;
  const allyCount = live.allies.slice(0, allyLimit).filter(Boolean).length;
  const enemyCount = live.enemies.filter(Boolean).length;
  const allyLabel = $("#live-ally-count");
  const enemyLabel = $("#live-enemy-count");
  if (allyLabel) allyLabel.textContent = `${allyCount}/${allyLimit}`;
  if (enemyLabel) {
    const extra =
      live.mode === "arena" && live.arenaEnemies?.length > live.enemies.length
        ? ` (+${live.arenaEnemies.length - live.enemies.length})`
        : "";
    enemyLabel.textContent = `${enemyCount}/5${extra}`;
  }
}

function renderInventory() {
  const host = $("#live-inventory");
  if (!host) return;
  const cap = maxSlots();
  const slots = [];
  for (let i = 0; i < cap; i += 1) {
    const item = itemById(live.owned[i]);
    if (item) {
      slots.push(`<div class="inv-slot filled" data-inv-index="${i}">
        <span class="inv-img-wrap">${avatarHTML(item, "", "item")}</span>
        <span class="inv-name">${esc(item.name)}</span>
        <span class="inv-cost">${num(item.gold)}</span>
        <button type="button" class="inv-x" data-inv-remove="${i}" aria-label="Quitar ${esc(
        item.name
      )}"><svg class="icon" style="width:9px;height:9px"><use href="#i-close"/></svg></button>
      </div>`);
    } else {
      const label =
        cap === 7 && i === 6 ? "Botas · role quest" : `Objeto ${i + 1}`;
      slots.push(`<button type="button" class="inv-slot" data-inv-add="${i}" aria-label="Añadir objeto">
        <svg class="icon"><use href="#i-plus"/></svg>
        <span class="inv-name">${label}</span>
      </button>`);
    }
  }
  host.classList.toggle("slots-7", cap === 7);
  host.innerHTML = slots.join("");
  const count = $("#live-slot-count");
  if (count) count.textContent = `${live.owned.length} / ${cap}`;
  const gold = $("#live-gold");
  if (gold && document.activeElement !== gold) gold.value = String(live.gold);
  const minute = $("#live-minute");
  if (minute && document.activeElement !== minute) minute.value = String(live.minute);
  const minuteLabel = $("#live-minute-label");
  if (minuteLabel) minuteLabel.textContent = String(live.minute);
}

function itemReasons(reasons, fallback = []) {
  const list = (reasons?.length ? reasons : fallback).slice(0, 3);
  return list.map((r) => `<span class="reason">${esc(r)}</span>`).join("");
}

function componentsHTML(components = []) {
  if (!components.length) return "";
  return `<div class="components">${components
    .slice(0, 3)
    .map(
      (c) => `<span class="comp ${c.affordable ? "ok" : ""}" title="${esc(c.name)} · ${num(c.gold)} oro">
        ${c.icon ? `<img src="${esc(iconUrl("item", c.icon))}" alt="" loading="lazy" data-fallback="hide">` : ""}
        ${esc(c.name)}
      </span>`
    )
    .join("")}</div>`;
}

function ownedComponentsHTML(components = []) {
  if (!components.length) return "";
  return `<div class="components">${components
    .slice(0, 5)
    .map(
      (c) => `<span class="comp ok" title="${esc(c.name)} · ${num(c.gold)} oro">
        ${c.icon ? `<img src="${esc(iconUrl("item", c.icon))}" alt="" loading="lazy" data-fallback="hide">` : ""}
        ${esc(c.name)}
      </span>`
    )
    .join("")}</div>`;
}

function legendaryCardHTML(target, options) {
  const owned = target.ownedComponents || [];
  const cost = Number(target.cost ?? target.missingGold) || 0;
  const missing = Number(target.missingGold) || 0;
  const freed = Number(target.freesSlots) || 0;
  const freeSlot = live.owned.length < maxSlots();
  const needsNewSlot = target.needsSlot && !freeSlot;
  const detail = target.needsSlot
    ? freeSlot
      ? "Ocupa un hueco libre"
      : "Necesitas un hueco libre"
    : freed > 0
      ? `Libera ${freed} hueco${freed === 1 ? "" : "s"}`
      : "Se completa en el mismo hueco";
  const priceLine = target.needsSlot
    ? `${num(cost)} oro${target.affordable ? "" : ` · te faltan ${num(missing)}`}`
    : `${num(cost)} oro para completar${target.affordable ? "" : ` · te faltan ${num(missing)}`}`;
  const badge = needsNewSlot
    ? `<span class="badge">Falta un hueco</span>`
    : target.affordable
      ? `<span class="badge teal">Listo para completar</span>`
      : `<span class="badge">Faltan ${num(missing)}</span>`;
  const optionsHTML = (options || []).length
    ? `<div class="legendary-options"><span class="label">Otras opciones</span>${options
        .slice(0, 2)
        .map((option) => {
          const optionSlotBlocked = option.needsSlot && live.owned.length >= maxSlots();
          const optionReady = option.affordable && !optionSlotBlocked;
          const optionCost = Number(option.cost ?? option.missingGold) || 0;
          return `<button type="button" class="legendary-option" data-buy="${option.id}" data-buy-gold="${optionCost}" data-buy-name="${esc(
            option.name
          )}" data-buy-consumes="${(option.ownedComponents || [])
            .map((component) => component.id)
            .join(",")}" ${optionReady ? "" : "disabled"}>
            ${avatarHTML(option, "sm", "item")}
            <strong>${esc(option.name)}</strong>
            <small>${
              optionSlotBlocked
                ? "Falta hueco"
                : option.affordable
                  ? "Completar"
                  : `Faltan ${num(option.missingGold)}`
            }</small>
          </button>`;
        })
        .join("")}</div>`
    : "";
  return `<div class="legendary-head">
      <span class="eyebrow">Objetivo legendario</span>
      ${badge}
    </div>
    <div class="legendary-body">
      <div class="legendary-main">
        ${avatarHTML(target, "lg", "item")}
        <div class="legendary-copy">
          <strong>${esc(target.name)}</strong>
          <p>${esc(detail)} · ${esc(priceLine)}</p>
          ${ownedComponentsHTML(owned) || `<p class="fineprint">Aún no tienes componentes para este objeto.</p>`}
        </div>
        <button type="button" class="btn tiny" data-buy="${target.id}" data-buy-gold="${cost}" data-buy-name="${esc(
          target.name
        )}" data-buy-consumes="${owned.map((component) => component.id).join(",")}" ${
          target.affordable && !needsNewSlot ? "" : "disabled"
        }>Completar</button>
      </div>
      ${optionsHTML}
    </div>`;
}

function renderLegendary(build) {
  const host = $("#live-legendary");
  if (!host) return;
  const target = build?.nextLegendary;
  if (!live.champion || !target) {
    host.classList.add("hidden");
    host.innerHTML = "";
    return;
  }
  host.innerHTML = legendaryCardHTML(target, build.legendaryOptions || []);
  host.classList.remove("hidden");
}

function renderItemCard(item, isNext = false) {
  const cap = maxSlots();
  const freeSupport =
    SUPPORT_ITEM_IDS.has(Number(item.id)) && Number(item.id) !== 3865;
  const isUpgrade = Boolean(item.upgrade);
  const disabled =
    !item.affordable ||
    (!isUpgrade && live.owned.length >= cap && !item.replacement);
  const label = item.replacement || freeSupport
    ? "Mejorar"
    : isUpgrade
      ? "Completar"
      : live.owned.length >= cap
        ? "Lleno"
        : item.affordable
          ? "Comprar"
          : "Sin oro";
  const actionNote = item.replacement
    ? "mejora gratuita"
    : freeSupport
      ? "mejora de quest"
      : isUpgrade
        ? item.affordable
          ? "completa el legendario"
          : `faltan ${num(item.missingGold)}`
        : item.affordable
          ? "disponible"
          : `faltan ${num(item.missingGold)}`;
  const signals = [
    isUpgrade ? "Completa legendario" : "",
    item.replacement ? "Role quest" : "",
    freeSupport ? "Soporte" : "",
    item.modelProb ? `BuildNet ${pct(item.modelProb, 1)}` : "",
    item.winrate ? `WR ${pct(item.winrate, 1)}` : "",
  ].filter(Boolean);
  const components = isUpgrade
    ? ownedComponentsHTML(item.ownedComponents)
    : componentsHTML(item.components);
  return `<article class="item-reco ${isNext ? "next-buy" : ""}">
    ${avatarHTML(item, "", "item")}
    <div class="item-reco-main">
      <div class="item-reco-title">
        <strong>${esc(item.name)}</strong>
        <span class="item-cost" title="${isUpgrade ? `${num(item.fullGold)} oro total` : ""}"><svg class="icon"><use href="#i-gold"/></svg>${num(item.gold)}</span>
        ${isNext ? `<span class="reason" style="color:var(--teal);border-color:rgba(10,200,185,.3)">${isUpgrade ? "Completar ahora" : "Compra ahora"}</span>` : ""}
      </div>
      <div class="item-reasons">${itemReasons(item.reasons, [`Score ${Math.round(item.score)}`])}</div>
      ${signals.length ? `<div class="item-signals">${signals.map((signal) => `<span>${esc(signal)}</span>`).join("")}</div>` : ""}
      ${components}
    </div>
    <div class="item-side">
      <strong>${Math.round(item.score)}</strong>
      <small>prioridad</small>
      <div class="score-bar"><i style="width:${Math.max(3, Math.min(100, item.score))}%"></i></div>
    </div>
    <div class="item-action">
      <button type="button" class="btn tiny" data-buy="${item.id}" data-buy-gold="${item.gold}" data-buy-name="${esc(item.name)}" data-buy-consumes="${(item.consumes || []).join(",")}" ${disabled ? "disabled" : ""}>${label}</button>
      <small>${actionNote}</small>
    </div>
  </article>`;
}

function renderBoots(build) {
  const host = $("#live-boots");
  if (!host) return;
  const boots = build?.bootsRecommendation;
  if (!live.champion || !boots) {
    host.classList.add("hidden");
    host.innerHTML = "";
    return;
  }
  const cap = maxSlots();
  const owned = boots.owned === true;
  const upgrade = boots.upgrade === true;
  const full = live.owned.length >= cap;
  const canBuy = !owned && boots.affordable && (upgrade || !full);
  const label = upgrade ? "Mejorar" : "Comprar";
  const action = owned
    ? "Ya las llevas"
    : full && !upgrade
      ? "Sin hueco"
      : boots.affordable
        ? label
        : `Faltan ${num(boots.missingGold)}`;
  const reasons = (boots.reasons || []).slice(0, 2).map((reason) => esc(reason)).join(" · ");
  host.innerHTML = `<div class="boots-card ${owned ? "owned" : ""}">
    <div class="boots-copy">
      <span class="eyebrow">Botas${owned ? " actuales" : " recomendadas"}</span>
      <div class="boots-main">
        ${avatarHTML(boots, "sm", "item")}
        <div>
          <strong>${esc(boots.name)}</strong>
          <p>${reasons || (owned ? "Ya las llevas equipadas" : "Siguiente compra de botas")}</p>
        </div>
        <span class="boots-price">${owned ? "—" : `${num(boots.gold)} oro`}</span>
        ${
          owned
            ? ""
            : `<button type="button" class="btn tiny" data-buy="${boots.id}" data-buy-gold="${
                boots.gold
              }" data-buy-name="${esc(boots.name)}" ${canBuy ? "" : "disabled"}>${action}</button>`
        }
      </div>
    </div>
  </div>`;
  host.classList.remove("hidden");
}

function damageClass(damage) {
  const key = String(damage || "").toLowerCase();
  if (key === "ad") return "ad";
  if (key === "ap") return "ap";
  if (key === "tanque") return "tank";
  if (key === "soporte") return "support";
  return "mixed";
}

function threatClass(threat) {
  const key = String(threat || "").toLowerCase();
  if (key === "fuerte") return "strong";
  if (key === "activo") return "active";
  return "normal";
}

function renderEnemyBuilds(build) {
  const host = $("#live-enemy-builds");
  if (!host) return;
  const enemyBuild = build?.gameAnalysis?.enemyBuild;
  const details = (enemyBuild?.details || []).filter((detail) => detail.items?.length);
  if (!live.champion || !details.length) {
    host.classList.add("hidden");
    host.innerHTML = "";
    return;
  }
  const chips = [];
  if (enemyBuild.armor > 0) chips.push(`Armadura ${num(enemyBuild.armor)}`);
  if (enemyBuild.magicResist > 0) chips.push(`RM ${num(enemyBuild.magicResist)}`);
  if (enemyBuild.health > 0) chips.push(`Vida ${num(enemyBuild.health)}`);
  if (enemyBuild.adThreats) chips.push(`${enemyBuild.adThreats} AD`);
  if (enemyBuild.apThreats) chips.push(`${enemyBuild.apThreats} AP`);
  const healingThreat = build?.gameAnalysis?.healingThreat || [];
  const healingNames = [...new Set([...(enemyBuild.healing || []), ...healingThreat])];
  if (healingNames.length) {
    chips.push(`Curación: ${healingNames.slice(0, 3).join(", ")}`);
  }
  if (enemyBuild.shieldPriority && enemyBuild.shields?.length) {
    chips.push(`Escudos: ${enemyBuild.shields.slice(0, 3).join(", ")}`);
  }
  host.innerHTML = `<div class="enemy-builds-card">
    <div class="enemy-builds-head">
      <span class="eyebrow">Builds rivales</span>
      <span class="badge">${details.length} leídas</span>
    </div>
    <div class="enemy-build-list">${details
      .map((detail) => {
        const items = (detail.items || [])
          .map((id) => itemById(id))
          .filter(Boolean)
          .slice(0, 6);
        const damage = esc(detail.damage || "Mixto");
        const threat = esc(detail.threat || "Normal");
        const kda = `${detail.kills ?? 0}/${detail.deaths ?? 0}/${detail.assists ?? 0}`;
        const champion = championByName(detail.champion);
        const championIcon = detail.icon || champion?.icon || "";
        return `<div class="enemy-build-row">
          <div class="enemy-build-champ">
            <img class="enemy-build-avatar" src="${iconUrl("champion", championIcon)}" alt="${esc(
              detail.champion,
            )}" />
            <div>
              <strong>${esc(detail.champion)}</strong>
              <small>${kda} · N${detail.level || 1}</small>
            </div>
          </div>
          <div class="enemy-build-items">${items
            .map(
              (item) =>
                `<span class="enemy-build-item" title="${esc(item.name)}"><img src="${iconUrl(
                  "item",
                  item.icon,
                )}" alt="${esc(item.name)}" /></span>`,
            )
            .join("")}</div>
          <div class="enemy-build-tags">
            <span class="enemy-build-tag damage-${damageClass(detail.damage)}">${damage}</span>
            ${
              detail.threat && detail.threat !== "Normal"
                ? `<span class="enemy-build-tag threat-${threatClass(detail.threat)}">${threat}</span>`
                : ""
            }
          </div>
        </div>`;
      })
      .join("")}</div>
    ${
      chips.length
        ? `<div class="enemy-builds-summary">${chips
            .map((chip) => `<span>${esc(chip)}</span>`)
            .join("")}</div>`
        : ""
    }
  </div>`;
  host.classList.remove("hidden");
}

function renderSituational(build) {
  const host = $("#live-situational");
  if (!host) return;
  const items = build?.situationalItems || [];
  if (!live.champion || !items.length) {
    host.classList.add("hidden");
    host.innerHTML = "";
    return;
  }
  const cap = maxSlots();
  const full = live.owned.length >= cap;
  host.innerHTML = `<div class="situational-card">
    <div class="situational-head">
      <span class="eyebrow">Según las builds rivales</span>
      <span class="badge gold">Ajuste</span>
    </div>
    <div class="situational-list">${items
      .slice(0, 2)
      .map((item) => {
        const ready = item.affordable && !full;
        const reason = item.contextNote || (item.reasons || [])[0] || "Compra situacional";
        return `<div class="situational-item">
          ${avatarHTML(item, "sm", "item")}
          <div class="situational-copy">
            <strong>${esc(item.name)}</strong>
            <p>${esc(reason)}</p>
          </div>
          <span class="situational-price">${num(item.gold)} oro</span>
          <button type="button" class="btn tiny" data-buy="${item.id}" data-buy-gold="${
            item.gold
          }" data-buy-name="${esc(item.name)}" ${ready ? "" : "disabled"}>${
            full ? "Sin hueco" : item.affordable ? "Comprar" : `Faltan ${num(item.missingGold)}`
          }</button>
        </div>`;
      })
      .join("")}</div>
  </div>`;
  host.classList.remove("hidden");
}

function renderItems(build) {
  const host = $("#live-items");
  const subtitle = $("#live-reco-subtitle");
  const badge = $("#live-mode");
  const status = $("#live-reco-status");
  if (!host) return;
  setModeBadge(badge, build?.mode);
  renderLegendary(build);
  renderBoots(build);
  renderEnemyBuilds(build);
  renderSituational(build);

  if (!live.champion) {
    host.innerHTML = emptyState("i-brain", "Elige tu campeón", "RiftSense necesita el campeón para buscar builds con contexto.");
    if (subtitle) subtitle.textContent = "Recomendaciones contextualizadas";
    if (status) status.textContent = "";
    lastAnnouncedPurchase = "";
    lastItemsSignature = "";
    renderMobileBuy(null);
    return;
  }
  if (build?.mode === "complete" && !build?.nextLegendary) {
    const cap = Number(build?.slotCap) || maxSlots();
    host.innerHTML = emptyState(
      "i-trophy",
      "Build completa",
      live.mode === "arena"
        ? "Ya tienes 6 objetos de Arena."
        : cap === 7
          ? "Ya tienes 6 objetos completos y las botas en la ranura de role quest."
          : "Ya tienes 6 objetos. Reserva oro para el trinket y consumibles."
    );
    if (subtitle) subtitle.textContent = "Inventario lleno";
    if (status) status.textContent = "";
    lastAnnouncedPurchase = "";
    lastItemsSignature = "";
    renderMobileBuy(null);
    return;
  }
  const items = build?.items || [];
  if (!items.length) {
    host.innerHTML = build?.nextLegendary
      ? `<p class="summary-empty">No hay más mejoras directas con los componentes actuales; vende uno para seguir la build.</p>`
      : emptyState("i-info", "Sin recomendaciones", "Añade rivales o revisa el inventario.");
    if (subtitle) subtitle.textContent = build?.nextLegendary ? "Inventario lleno · objetivo legendario" : "Sin candidatos";
    if (status) status.textContent = "";
    lastAnnouncedPurchase = "";
    lastItemsSignature = "";
    renderMobileBuy(build);
    return;
  }
  if (subtitle) {
    const component = build?.nextComponent
      ? ` · componente: ${build.nextComponent.name}`
      : "";
    const modeText = build?.modeLabel ? `${build.modeLabel} · ` : "";
    const strategy =
      build.recommendationSource === "antiheal"
        ? "antiheal + BuildNet"
        : build.recommendationMode === "model_first"
          ? "BuildNet + winrate histórico"
          : build.mode === "model"
            ? "modelo + datos"
            : "datos pro";
    const cap = Number(build?.slotCap) || maxSlots();
    const full =
      build?.fullInventory && build?.nextLegendary
        ? " · Inventario lleno: completa un legendario"
        : "";
    subtitle.textContent = `${modeText}Slot ${build.slot} de ${cap} · ${items.length} candidatos · ${strategy}${component}${full}`;
  }

  const nextId = build?.nextPurchase?.id;
  const cards = [];
  if (build?.nextPurchase) {
    cards.push(renderItemCard(build.nextPurchase, true));
  }
  for (const item of items) {
    if (item.id !== nextId) cards.push(renderItemCard(item, false));
  }
  const signature = JSON.stringify({
    next: nextId,
    gold: build?.gold,
    slot: build?.slot,
    cap: build?.slotCap,
    mode: build?.mode,
    source: build?.recommendationSource,
    component: build?.nextComponent?.id || 0,
    fullInventory: Boolean(build?.fullInventory),
    legendary: build?.nextLegendary?.id || 0,
    legendaryMissing: build?.nextLegendary?.missingGold || 0,
    legendaryAffordable: Boolean(build?.nextLegendary?.affordable),
    legendaryOptions: (build?.legendaryOptions || []).map((option) => option.id),
    expanded: itemsExpanded,
    owned: live.owned,
    items: items.map((item) => [
      item.id,
      Math.round(item.score || 0),
      Boolean(item.affordable),
      Boolean(item.replacement),
      item.replaces || 0,
    ]),
  });
  if (signature === lastItemsSignature) {
    renderMobileBuy(build);
    return;
  }
  lastItemsSignature = signature;
  const visible = itemsExpanded ? cards : cards.slice(0, 4);
  const hidden = cards.length - visible.length;
  const more =
    cards.length > 4
      ? `<button type="button" class="btn ghost tiny item-more" data-items-more aria-expanded="${itemsExpanded}">${
          itemsExpanded ? "Ver menos" : `Ver ${hidden} alternativas`
        }</button>`
      : "";
  host.innerHTML = visible.join("") + more;

  const purchase = build?.nextPurchase;
  if (status) {
    const target = build?.nextLegendary;
    const message = purchase
      ? `Siguiente compra: ${purchase.name}, ${num(purchase.gold)} de oro.`
      : target
        ? `Objetivo legendario: ${target.name}, te faltan ${num(target.missingGold)} de oro.`
        : "";
    if (message && message !== lastAnnouncedPurchase) {
      status.textContent = message;
      lastAnnouncedPurchase = message;
    } else if (!message) {
      status.textContent = "";
      lastAnnouncedPurchase = "";
    }
  }
  renderMobileBuy(build);
}

function renderMobileBuy(build) {
  const host = $("#live-mobile-buy");
  if (!host) return;
  const item = build?.nextPurchase;
  const consumes = (item?.consumes || []).join(",");
  const signature =
    live.champion && item && build?.mode !== "complete"
      ? `${item.id}|${item.gold}|${item.affordable}|${item.replacement}|${item.replaces}|${item.upgrade}|${consumes}`
      : "none";
  if (signature === lastMobileBuySignature) return;
  lastMobileBuySignature = signature;
  if (signature === "none") {
    host.classList.add("hidden");
    host.innerHTML = "";
    return;
  }
  const label = item.replacement ? "Mejorar" : item.upgrade ? "Completar" : "Comprar";
  host.innerHTML = `
    <div class="live-mobile-buy-main">
      ${avatarHTML(item, "", "item")}
      <div class="live-mobile-buy-copy">
        <small>${item.upgrade ? "Completar legendario" : "Próxima compra"}</small>
        <strong>${esc(item.name)}</strong>
      </div>
    </div>
    <div class="live-mobile-buy-side">
      <span>${num(item.gold)} oro</span>
      <button type="button" class="btn tiny" data-buy="${item.id}" data-buy-gold="${item.gold}" data-buy-name="${esc(item.name)}" data-buy-consumes="${consumes}" ${
        item.affordable ? "" : "disabled"
      }>${label}</button>
    </div>`;
  host.classList.remove("hidden");
}

function renderCoach(build) {
  const host = $("#live-coach");
  if (!host) return;
  const analysis = build?.gameAnalysis;
  if (!analysis) {
    host.classList.add("hidden");
    host.innerHTML = "";
    return;
  }

  const notes = (analysis.notes || []).slice(0, 3).join(" · ");
  const extras = [];
  if (analysis.enemyFed?.length) extras.push(`Fuertes: ${analysis.enemyFed.join(", ")}`);
  if (analysis.healingThreat?.length) extras.push(`Curación: ${analysis.healingThreat.join(", ")}`);
  const metrics = [
    `KDA ${analysis.kills}/${analysis.deaths}/${analysis.assists}`,
    analysis.showCs !== false ? `${num(analysis.cs)} CS · ${analysis.csPerMin}/min` : "",
    `Nivel ${analysis.level}`,
    analysis.showTeamStats !== false
      ? `Equipos ${analysis.teamKills}-${analysis.enemyKills}`
      : "",
    analysis.showTeamStats !== false
      ? `Niveles ${analysis.teamLevel}-${analysis.enemyLevel}`
      : "",
  ].filter(Boolean);

  host.className = `live-coach ${analysis.state || ""}`;
  host.innerHTML = `
    <div class="coach-copy">
      <span class="eyebrow">${esc(analysis.modeLabel || "Lectura de partida")}</span>
      <strong>${esc(analysis.label)}</strong>
      <div class="coach-metrics">${metrics.map((metric) => `<span>${esc(metric)}</span>`).join("")}</div>
      <p>${esc(notes)}${extras.length ? ` · ${esc(extras.join(" · "))}` : ""}</p>
    </div>`;
  host.classList.remove("hidden");
}

function renderVerdict(draft, build) {
  const champ = championByName(live.champion);
  const title = $("#live-verdict-title");
  const text = $("#live-verdict-text");
  const pills = $("#live-pills");
  const probability = draft?.winProbability ?? null;
  const mode = build?.gameMode || live.mode || "classic";
  const classic = mode === "classic";
  const info = MODE_INFO[mode] || MODE_INFO.classic;
  const confidence = confidenceFor(build);
  const confidencePill =
    build?.mode === "complete" && !build?.nextLegendary
      ? `<span class="pill teal">Build completa</span>`
      : `<span class="pill ${confidence.tone}">Confianza ${confidence.label}</span>`;
  $("#live-gauge")?.classList.toggle("hidden", !classic);

  if (!champ) {
    if (title) title.textContent = classic ? "Configura tu partida" : `${modeLabel(mode)} · configura tu partida`;
    if (text) text.textContent = info.empty;
    if (pills) {
      pills.innerHTML = classic
        ? ""
        : MODES.map(
            (entry) =>
              `<span class="pill ${entry.key === mode ? "gold" : ""}">${entry.label}</span>`
          ).join("");
    }
    return;
  }

  if (!classic) {
    if (title) title.textContent = `${build?.modeLabel || modeLabel(mode)} · ${champ.name}`;
    if (text) text.textContent = info.champion;
    if (pills) {
      const profile = build?.enemyProfile;
      const analysis = build?.gameAnalysis;
      pills.innerHTML = [
        `<span class="pill gold">${esc(build?.modeLabel || modeLabel(mode))}</span>`,
        `<span class="pill">${live.owned.length}/${Number(build?.slotCap) || maxSlots()} objetos</span>`,
        confidencePill,
        analysis
          ? `<span class="pill ${analysis.state === "ahead" ? "teal" : analysis.state === "behind" ? "bad" : ""}">${esc(
              analysis.label
            )}</span>`
          : "",
        analysis ? `<span class="pill">KDA ${analysis.kills}/${analysis.deaths}/${analysis.assists}</span>` : "",
        analysis?.showCs !== false && analysis
          ? `<span class="pill">${num(analysis.cs)} CS</span>`
          : "",
        profile
          ? `<span class="pill">${profile.label === "Mixta" ? "Daño mixto" : `Rival ${profile.label}`}</span>`
          : "",
      ]
        .filter(Boolean)
        .join("");
    }
    return;
  }

  renderGauge($("#live-gauge"), probability);
  const p = probability ?? 0.5;
  if (title) {
    title.textContent = p >= 0.54 ? "Draft favorable" : p >= 0.47 ? "Draft parejo" : "Draft cuesta arriba";
  }
  if (text) {
    const base = `La estimación del draft es ${pct(p, 1)} de victoria para tu lado.`;
    const incomplete = draft && !draft.complete ? ` Faltan ${10 - (draft.knownPicks || 0)} picks por conocer.` : "";
    text.textContent = base + incomplete;
  }
  if (pills) {
    const profile = build?.enemyProfile;
    const analysis = build?.gameAnalysis;
    pills.innerHTML = [
      `<span class="pill">${esc(roleLabel(live.role))}</span>`,
      `<span class="pill gold">${live.owned.length}/${Number(build?.slotCap) || maxSlots()} objetos</span>`,
      confidencePill,
      analysis
        ? `<span class="pill ${analysis.state === "ahead" ? "teal" : analysis.state === "behind" ? "bad" : ""}">${esc(
            analysis.label
          )}</span>`
        : "",
      analysis ? `<span class="pill">KDA ${analysis.kills}/${analysis.deaths}/${analysis.assists}</span>` : "",
      analysis ? `<span class="pill">${num(analysis.cs)} CS</span>` : "",
      profile ? `<span class="pill">${profile.label === "Mixta" ? "Daño mixto" : `Rival ${profile.label}`}</span>` : "",
      `<span class="pill teal">${draft?.mode === "model" ? "DraftNet" : "Datos pro"}</span>`,
    ]
      .filter(Boolean)
      .join("");
  }
}

function renderDamage(profile) {
  const host = $("#live-damage-profile");
  if (!host) return;
  const physical = profile?.physical ?? 0.5;
  const magic = profile?.magic ?? 0.5;
  const ad = host.querySelector(".damage-bar .ad");
  const ap = host.querySelector(".damage-bar .ap");
  if (ad) ad.style.width = `${physical * 100}%`;
  if (ap) ap.style.width = `${magic * 100}%`;
  const legend = host.querySelector(".damage-legend");
  if (legend) {
    legend.innerHTML = `<span>AD ${pct(physical, 0)}</span><span>AP ${pct(magic, 0)}</span>`;
  }
}

function renderPlan(build) {
  const host = $("#build-plan");
  if (!host) return;
  const plan = build?.buildPlan || [];
  if (!plan.length) {
    host.innerHTML = emptyState(
      "i-gold",
      live.champion ? "Sin plan adicional" : "Elige un campeón",
      live.champion ? "Añade oro y objetos para simular la ruta." : "El plan aparecerá aquí."
    );
    return;
  }
  const total = plan.reduce((sum, item) => sum + (item.gold || 0), 0);
  host.innerHTML =
    `<div class="build-plan-track">` +
    plan
      .map(
        (item, index) => `${index ? `<span class="plan-step arrow" aria-hidden="true">→</span>` : ""}
      <div class="plan-step ${index === 0 ? "current" : ""}">
        <span class="plan-num">${item.slot}</span>
        ${avatarHTML(item, "", "item")}
        <strong>${esc(item.name)}</strong>
        <small>${num(item.gold)}</small>
      </div>`
      )
      .join("") +
    `</div><div class="plan-cost">Coste del plan: <b>${num(total)}</b> oro ${
      live.mode === "arena"
        ? "(objetos completos con precios de Arena)"
        : "(sin contar lo ya comprado)"
    }</div>`;
}

function renderNotes(build, draft) {
  
  const host = $("#live-notes");
  if (!host) return;
  if (!live.champion) {
    host.innerHTML = emptyState("i-info", "Sin datos todavía", "Completa el estado para ver el perfil del rival y los spikes.");
    return;
  }
  const rows = [];
  const mode = build?.gameMode || live.mode || "classic";
  const classic = mode === "classic";
  const nearby = build?.powerSpike?.nearby || {};
  const entries = Object.entries(nearby)
    .map(([slot, value]) => [Number(slot), value])
    .sort((a, b) => a[0] - b[0]);
  if (build?.recommendationMode === "model_first") {
    const source =
      build.recommendationSource === "antiheal"
        ? " La compra principal es antiheal porque coincide con la mejor opción del modelo."
        : "";
    rows.push(`<div class="note-box"><strong>Prioridad de compra</strong>BuildNet y el winrate histórico deciden la compra principal; el contexto solo ajusta alternativas.${source}</div>`);
  }
  if (build?.antihealSuggestion) {
    const suggestion = build.antihealSuggestion;
    rows.push(`<div class="note-box warn"><strong>Antiheal disponible</strong>${esc(
      suggestion.name
    )} cuesta ${num(suggestion.gold)}. Es una sugerencia situacional para la curación enemiga; no sustituye la compra principal.</div>`);
  }
  if (build?.mode === "complete" && !build?.nextLegendary) {
    const cap = Number(build?.slotCap) || maxSlots();
    rows.push(
      `<div class="note-box good"><strong>Build completa</strong>${
        live.mode === "arena"
          ? "Ya tienes 6 objetos de Arena."
          : cap === 7
            ? "Ya tienes 6 objetos completos y las botas en la ranura de role quest."
            : "Ya tienes 6 objetos. Reserva oro para el trinket y consumibles."
      }</div>`
    );
  } else {
    const confidence = confidenceFor(build);
    rows.push(`<div class="note-box"><strong>Confianza de la recomendación: ${confidence.label} (${Math.round(
      confidence.score * 100
    )}%)</strong>${confidence.reasons.map((reason) => esc(reason)).join("<br>")}</div>`);
  }
  if (build?.modeNotes?.length) {
    rows.push(`<div class="note-box mode-note"><strong>${esc(build.modeLabel || modeLabel(mode))}</strong>${build.modeNotes
      .map((note) => esc(note))
      .join("<br>")}</div>`);
  }
  if (classic && live.enemyRolesInferred) {
    rows.push(`<div class="note-box"><strong>Roles rivales</strong>El cliente no expone los roles del equipo rival. Se estiman por campeón y composición; aparecen marcados con ≈.</div>`);
  }
  if (classic && live.role === "BOT" && maxSlots() === 7) {
    rows.push(`<div class="note-box good"><strong>Role quest del bot completada</strong>Las botas ocupan la septima ranura dedicada: puedes llevar 6 objetos completos ademas de las botas. Antes de completar la quest, las botas siguen ocupando una ranura normal.</div>`);
  }
  if (classic && live.role === "SUPPORT") {
    rows.push(`<div class="note-box"><strong>Objeto de soporte</strong>World Atlas y sus mejoras ocupan una ranura normal de inventario. La role quest de support anade una ranura aparte para wards de control, no para el objeto de soporte.</div>`);
  }
  if (live.owned.length >= 6 && mode !== "classic") {
    const starter = live.owned
      .map((id) => itemById(id))
      .find((item) => item && (isArenaStarter(item.id) || (item.tags || []).includes("Lane") && item.gold <= 1000));
    if (starter) {
      rows.push(`<div class="note-box warn"><strong>Slot completo</strong>Puedes vender ${esc(
        starter.name
      )} para liberar un slot y comprar un objeto completo.</div>`);
    }
  }
  if (classic && entries.length) {
    rows.push(`<div class="note-box"><strong>Winrate por número de objetos</strong>
      <table class="spike-table"><thead><tr><th>Objetos</th><th>Partidas</th><th>WR pro</th></tr></thead><tbody>
      ${entries
        .map(
          ([slot, value]) =>
            `<tr class="${slot === build?.powerSpike?.slot ? "me" : ""}"><td>${slot}</td><td>${num(
              value.n
            )}</td><td>${pct(value.winrate, 1)}</td></tr>`
        )
        .join("")}
      </tbody></table>
      <small style="color:var(--muted-2);font-size:9.5px">Asociación, no causalidad. Muestra del dataset pro.</small>
    </div>`);
  }

  const profile = build?.enemyProfile;
  if (profile) {
    const missing =
      classic && profile.known < 5
        ? "Faltan campeones enemigos, el cálculo es aproximado. "
        : "";
    const knownText = classic ? "" : `Se usan ${profile.known || 0} rivales conocidos. `;
    rows.push(`<div class="note-box"><strong>Perfil del rival</strong>${missing}${knownText}La composición rival es ${
      profile.label === "Mixta" ? "de daño mixto" : `principalmente ${profile.label}`
    } (${pct(profile.physical, 0)} físico / ${pct(profile.magic, 0)} mágico).</div>`);
  }
  if (classic && draft) {
    rows.push(`<div class="note-box ${draft.complete ? "good" : "warn"}"><strong>Draft ${
      draft.complete ? "completo" : "incompleto"
    }</strong>${pct(draft.winProbability, 1)} de victoria estimada. ${
      draft.complete ? "La estimación usa los 10 picks." : `Faltan ${10 - (draft.knownPicks || 0)} picks.`
    }</div>`);
  }

  if (classic && state.draft.bans.length) {
    rows.push(`<div class="note-box"><strong>Bans del draft</strong>Se están usando los ${state.draft.bans.length} bans configurados en la pestaña Draft.</div>`);
  }

  if (mode === "arena" || mode === "mayhem") {
    const bits = [];
    if (mode === "arena") {
      const trio = live.allies.slice(0, 3).filter(Boolean);
      bits.push(`Trío: ${trio.length ? trio.join(", ") : "sin completar"}`);
    }
    bits.push(`Augments: ${live.augments.length ? live.augments.join(", ") : "sin registrar"}`);
    if (mode === "arena") {
      const anvils = live.anvils.map((type) => ANVIL_LABELS[type] || type);
      bits.push(`Anviles: ${anvils.length ? anvils.join(", ") : "sin registrar"}`);
    }
    rows.push(`<div class="note-box"><strong>Corrección manual</strong>${bits
      .map((bit) => esc(bit))
      .join("<br>")}<br><small style="color:var(--muted-2)">La API local no expone estos datos; se guardan en el resumen de la partida.</small></div>`);
  }

  const notes = build?.notes || [];
  if (notes.length) {
    rows.push(`<div class="note-box"><strong>Notas del motor</strong>${notes.map((n) => esc(n)).join("<br>")}</div>`);
  }

  const scope =
    mode === "arena"
      ? "Arena: se leen KDA, nivel, oro e inventario cada 3 segundos. No se usan CS ni equipos porque la API local no expone la estructura de tríos."
      : mode === "classic"
        ? "Con Auto activo, RiftSense lee el estado de la partida cada 3 segundos (KDA, CS, niveles, equipo, oro e inventario) y adapta la siguiente compra. Sin partida en curso puedes introducir los datos manualmente. Nada de esto envía compras al juego."
        : `${modeLabel(mode)}: se leen KDA, CS, niveles, equipo, oro e inventario cada 3 segundos y se adapta la siguiente compra. La API local no expone los augments elegidos. Nada de esto envía compras al juego.`;
  rows.push(`<div class="note-box"><strong>Alcance</strong>${scope}</div>`);
  host.innerHTML = rows.join("");
}

/* ------------------------------------------------------------------ */
/* Request                                                             */
/* ------------------------------------------------------------------ */
function payload() {
  const enemies =
    live.mode === "arena"
      ? [...new Set([...live.enemies, ...(live.arenaEnemies || [])].filter(Boolean))]
      : live.enemies;
  return {
    champion: live.champion,
    mode: live.mode,
    role: live.role,
    side: live.side,
    allies: live.allies,
    enemies,
    owned: live.owned,
    gold: Number(live.gold) || 0,
    minute: Number(live.minute) || 0,
    stable: true,
    game: live.game,
    targetItem: live.targetLegendary || null,
    bans: state.draft.bans,
    limit: 10,
    simulate: true,
  };
}

async function refresh(immediate = false) {
  if (!state.meta) return;
  if (!live.champion) {
    lastData = null;
    renderVerdict(null, null);
    renderItems(null);
    renderCoach(null);
    renderPlan(null);
    renderNotes(null, null);
    renderDamage(null);
    return;
  }
  const seq = ++requestSeq;
  controller?.abort();
  controller = new AbortController();
  if (!lastData) {
    const host = $("#live-items");
    if (host) skeletonList(host, 4);
  }
  try {
    const data = await api.post("/api/live", payload(), { signal: controller.signal });
    if (seq !== requestSeq) return;
    lastData = data;
    const build = data.build;
    if (
      live.targetLegendary &&
      (build?.targetStatus === "completed" || build?.targetStatus === "invalid")
    ) {
      live.targetLegendary = null;
    }
    if (!live.targetLegendary && build?.nextLegendary?.id) {
      live.targetLegendary = build.nextLegendary.id;
    }
    if (live.tracking) {
      recordSession({
        champion: live.champion,
        mode: live.mode,
        role: live.role,
        side: live.side,
        minute: live.minute,
        owned: live.owned,
        build: data.build,
        augments: live.augments,
        anvils: live.anvils,
        allies: live.allies,
        kda: live.game,
        gameId: live.gameId,
      });
    }
    renderVerdict(data.draft, data.build);
    renderItems(data.build);
    renderCoach(data.build);
    renderPlan(data.build);
    renderDamage(data.build?.enemyProfile);
    renderNotes(data.build, data.draft);
  } catch (err) {
    if (err.name === "AbortError" || seq !== requestSeq) return;
    toast(err.message || "No se pudo actualizar la build", "error");
  }
}

const debouncedRefresh = (() => {
  let timer;
  return () => {
    clearTimeout(timer);
    timer = setTimeout(() => refresh(), 280);
  };
})();

/* ------------------------------------------------------------------ */
/* Eventos                                                             */
/* ------------------------------------------------------------------ */
function renderAll() {
  renderModes();
  renderRoles();
  renderSide();
  renderChampionSelect();
  renderMiniSlots();
  renderManual();
  renderInventory();
  applyModeLayout();
}

export function init() {
  if (bound) return;
  bound = true;
  renderAll();
  renderVerdict(null, null);
  renderItems(null);
  renderCoach(null);
  renderPlan(null);
  renderNotes(null, null);

  $("#live-modes")?.addEventListener("click", (event) => {
    const button = event.target.closest("[data-live-mode]");
    if (button) setMode(button.dataset.liveMode);
  });

  $("#live-roles")?.addEventListener("click", (event) => {
    const button = event.target.closest("[data-live-role]");
    if (button) changeRole(button.dataset.liveRole);
  });

  $("#live-augment-add")?.addEventListener("click", addAugment);
  $("#live-augment-input")?.addEventListener("keydown", (event) => {
    if (event.key === "Enter") {
      event.preventDefault();
      addAugment();
    }
  });
  $("#live-augment-chips")?.addEventListener("click", (event) => {
    const remove = event.target.closest("[data-augment-remove]");
    if (remove) removeAugment(Number(remove.dataset.augmentRemove));
  });
  $("#live-anvils")?.addEventListener("click", (event) => {
    const button = event.target.closest("[data-anvil]");
    if (button) addAnvil(button.dataset.anvil);
  });
  $("#live-anvil-chips")?.addEventListener("click", (event) => {
    const remove = event.target.closest("[data-anvil-remove]");
    if (remove) removeAnvil(Number(remove.dataset.anvilRemove));
  });

  $("#live-side")?.addEventListener("click", (event) => {
    const button = event.target.closest("button[data-side]");
    if (!button || button.dataset.side === live.side) return;
    live.side = button.dataset.side;
    renderSide();
    refresh();
  });

  $("#live-champ")?.addEventListener("click", async () => {
    const current = live.champion;
    const exclude = [
      ...live.allies.filter((n) => n && n !== current),
      ...live.enemies.filter(Boolean),
    ];
    const name = await openChampionPicker({
      role: live.mode === "classic" ? live.role : "ALL",
      exclude,
      title: "Tu campeón",
      kicker: "Estado de la partida",
    });
    if (name) setChampion(name);
  });

  for (const selector of ["#live-allies", "#live-enemies"]) {
    $(selector)?.addEventListener("click", async (event) => {
      const slot = event.target.closest("[data-mini-team]");
      if (!slot) return;
      const team = slot.dataset.miniTeam;
      const index = Number(slot.dataset.index);
      const classic = live.mode === "classic";
      const role = classic ? meta.roles[index] : null;
      const current = live[team][index];
      if (team === "allies" && index === ownIndex()) {
        const name = await openChampionPicker({
          role: live.mode === "classic" ? live.role : "ALL",
          exclude: [...live.allies.filter((n) => n && n !== live.champion), ...live.enemies.filter(Boolean)],
          title: "Tu campeón",
          kicker: "Estado de la partida",
        });
        if (name) setChampion(name);
        return;
      }
      const exclude = [
        live.champion,
        ...live.allies.filter((n) => n && n !== current),
        ...live.enemies.filter((n) => n && n !== current),
      ].filter(Boolean);
      const name = await openChampionPicker({
        role: role || "ALL",
        exclude,
        title: classic
          ? `${team === "allies" ? "Aliado" : "Rival"} · ${roleLabel(role)}`
          : `${team === "allies" ? "Aliado" : "Rival"} ${index + 1} · ${modeLabel()}`,
        kicker: "Composición",
      });
      if (!name) return;
      live[team][index] = name;
      if (live.mode === "arena" && team === "enemies") {
        if (index < live.arenaEnemies.length) live.arenaEnemies[index] = name;
        else live.arenaEnemies.push(name);
      }
      renderMiniSlots();
      refresh();
    });
  }

  $("#live-inventory")?.addEventListener("click", async (event) => {
    const remove = event.target.closest("[data-inv-remove]");
    if (remove) {
      removeItem(Number(remove.dataset.invRemove));
      return;
    }
    const add = event.target.closest("[data-inv-add]");
    if (!add) return;
    const id = await openItemPicker({
      exclude: live.owned,
      title: "Añadir objeto al inventario",
    });
    if (id != null) addItem(id);
  });

  const buyFromButton = (button) => {
    if (!button || button.disabled) return;
    const consumes = (button.dataset.buyConsumes || "")
      .split(",")
      .filter(Boolean)
      .map(Number);
    buyItem(
      Number(button.dataset.buy),
      Number(button.dataset.buyGold),
      button.dataset.buyName || "",
      consumes
    );
  };

  $("#live-items")?.addEventListener("click", (event) => {
    const more = event.target.closest("[data-items-more]");
    if (more) {
      itemsExpanded = !itemsExpanded;
      renderItems(lastData?.build);
      return;
    }
    buyFromButton(event.target.closest("[data-buy]"));
  });

  $("#live-legendary")?.addEventListener("click", (event) => {
    buyFromButton(event.target.closest("[data-buy]"));
  });

  $("#live-boots")?.addEventListener("click", (event) => {
    buyFromButton(event.target.closest("[data-buy]"));
  });

  $("#live-situational")?.addEventListener("click", (event) => {
    buyFromButton(event.target.closest("[data-buy]"));
  });

  $("#live-mobile-buy")?.addEventListener("click", (event) => {
    buyFromButton(event.target.closest("[data-buy]"));
  });

  $("#live-gold")?.addEventListener("input", (event) => {
    live.gold = Math.max(0, Math.min(100000, Number(event.target.value) || 0));
    debouncedRefresh();
  });

  $$(".gold-quick").forEach((button) => {
    button.addEventListener("click", () => {
      if (button.hasAttribute("data-gold-reset")) {
        live.gold = 0;
      } else {
        live.gold = Math.max(0, live.gold + Number(button.dataset.goldAdd || 0));
      }
      const input = $("#live-gold");
      if (input) input.value = String(live.gold);
      refresh();
    });
  });

  $("#live-minute")?.addEventListener("input", (event) => {
    live.minute = Number(event.target.value) || 0;
    const label = $("#live-minute-label");
    if (label) label.textContent = String(live.minute);
    debouncedRefresh();
  });

  $("#live-example")?.addEventListener("click", () => {
    const mode = live.mode;
    const arena = mode === "arena";
    const aram = mode === "aram" || mode === "mayhem";
    live.champion = "Ahri";
    live.role = "MID";
    live.side = "blue";
    live.allies = Array.from({ length: 5 }, (_, index) => (index === 0 ? "Ahri" : null));
    live.enemies = ["Gnar", "Vi", "Orianna", "Kai'Sa", "Nautilus"]
      .map((n) => (championByName(n) ? n : null));
    live.arenaEnemies = arena
      ? ["Gnar", "Vi", "Orianna", "Kai'Sa", "Nautilus", "Aatrox", "Rakan", "Sejuani"]
          .map((n) => (championByName(n) ? n : null))
          .filter(Boolean)
      : [];
    live.owned = (arena
      ? ["Guardian's Orb", "Sorcerer's Shoes"]
      : ["Doran's Ring", "Sorcerer's Shoes"])
      .map((n) => itemByName(n)?.id)
      .filter((id) => id != null);
    live.augments = arena ? ["Blade Waltz", "Goliath"] : live.mode === "mayhem" ? ["Dashing"] : [];
    live.anvils = arena ? ["stats"] : [];
    live.tracking = false;
    live.targetLegendary = null;
    live.enemyRolesInferred = false;
    live.gold = arena ? 2500 : 1600;
    live.minute = arena ? 8 : 9;
    itemsExpanded = false;
    lastAnnouncedPurchase = "";
    lastItemsSignature = "";
    lastMobileBuySignature = "";
    live.game = {
      kills: 4,
      deaths: 2,
      assists: 5,
      cs: arena ? 0 : 120,
      level: 10,
      gameTime: arena ? 600 : 540,
      teamKills: arena ? 0 : 10,
      enemyKills: arena ? 0 : 7,
      teamCs: arena ? 0 : 500,
      enemyCs: arena ? 0 : 420,
      teamLevel: arena ? 0 : 9.6,
      enemyLevel: arena ? 0 : 9.1,
      enemyFed: arena ? ["Aatrox", "Kai'Sa"] : ["Vi"],
      enemyBuilds: [],
      gameMode: arena ? "CHERRY" : mode === "mayhem" ? "KIWI" : aram ? "ARAM" : "CLASSIC",
      mapName: arena ? "Map30" : aram ? "Map12" : "Map11",
      queueId: arena ? 1700 : mode === "mayhem" ? 2400 : aram ? 450 : 420,
    };
    const source = $("#live-source");
    if (source) {
      source.textContent = `Fuente manual · ejemplo ${modeLabel(mode)}`;
      source.classList.remove("live");
    }
    renderAll();
    refresh(true);
    toast(`Ejemplo cargado: ${modeLabel(mode)} con Ahri.`, "success");
  });

  $("#live-reset")?.addEventListener("click", () => {
    live.champion = null;
    live.mode = "classic";
    live.modeLabel = "Grieta del Invocador";
    live.role = "MID";
    live.side = "blue";
    live.allies = [null, null, null, null, null];
    live.enemies = [null, null, null, null, null];
    live.arenaEnemies = [];
    live.arenaTeamKnown = false;
    live.owned = [];
    live.roleQuestBoots = null;
    live.augments = [];
    live.anvils = [];
    live.tracking = false;
    live.gameId = null;
    live.targetLegendary = null;
    live.enemyRolesInferred = false;
    itemsExpanded = false;
    lastAnnouncedPurchase = "";
    lastItemsSignature = "";
    lastMobileBuySignature = "";
    live.gold = 0;
    live.minute = 6;
    live.game = {
      kills: 0,
      deaths: 0,
      assists: 0,
      cs: 0,
      level: 1,
      gameTime: 0,
      teamKills: 0,
      enemyKills: 0,
      teamCs: 0,
      enemyCs: 0,
      teamLevel: 0,
      enemyLevel: 0,
      enemyFed: [],
      enemyBuilds: [],
      gameMode: "",
      mapName: "",
      queueId: null,
    };
    lastData = null;
    const source = $("#live-source");
    if (source) {
      source.textContent = "Fuente manual · introduce los datos";
      source.classList.remove("live");
    }
    renderAll();
    refresh(true);
    $("#live-gold").value = "0";
    $("#live-minute").value = "6";
    $("#live-minute-label").textContent = "6";
  });
}

/* ------------------------------------------------------------------ */
/* Integracion con el cliente local                                    */
/* ------------------------------------------------------------------ */
export function stopTracking() {
  live.tracking = false;
  live.targetLegendary = null;
}

export async function applyClientState(mapped) {
  if (!mapped?.available) return false;
  const champion = championByName(mapped.champion);
  if (!champion) return false;

  const pad = (list) =>
    Array.from({ length: 5 }, (_, index) => {
      const name = Array.isArray(list) ? list[index] : null;
      return name && championByName(name) ? name : null;
    });

  const mode =
    normalizeMode(mapped.mode) ||
    normalizeMode(mapped.gameMode) ||
    normalizeMode(mapped.mapName) ||
    "classic";
  const previousMode = live.mode;
  const previousChampion = live.champion;
  const previousGameId = live.gameId;
  const nextGameId = Number(mapped.gameId) || null;
  live.mode = mode;
  live.tracking = true;
  live.enemyRolesInferred = mode === "classic" && mapped.enemyRolesInferred === true;
  live.gameId = nextGameId || live.gameId || null;
  if (
    previousMode !== mode ||
    previousChampion !== champion.name ||
    (previousGameId && nextGameId && Number(previousGameId) !== nextGameId)
  ) {
    live.augments = [];
    live.anvils = [];
    live.targetLegendary = null;
    live.gameId = Number(mapped.gameId) || null;
  }
  live.modeLabel = mapped.modeLabel || modeLabel(mode);
  live.champion = champion.name;
  live.role = meta.roles.includes(mapped.role) ? mapped.role : live.role;
  live.side = mapped.side === "red" ? "red" : "blue";
  const mappedAllies = Array.isArray(mapped.allies) ? mapped.allies.filter(Boolean) : [];
  const mappedEnemies = Array.isArray(mapped.enemies) ? mapped.enemies.filter(Boolean) : [];
  live.allies = pad(mappedAllies);
  live.enemies = pad(mappedEnemies);
  live.arenaEnemies = mode === "arena" ? mappedEnemies : [];
  live.arenaTeamKnown = mode === "arena" && mapped.arenaTeamKnown === true;
  const championSlot = ownIndex();
  if (!live.allies[championSlot]) live.allies[championSlot] = champion.name;

  for (const item of Array.isArray(mapped.items) ? mapped.items : []) {
    if (!item?.id || meta.itemsById.has(item.id)) continue;
    meta.itemsById.set(item.id, {
      id: item.id,
      name: item.name || String(item.id),
      icon: `${item.id}.png`,
      gold: Number(item.price) || 0,
      tags: [],
    });
  }
  const mappedOwned = (Array.isArray(mapped.owned) ? mapped.owned : []).filter((id) =>
    itemById(id)
  );
  live.minute = Math.max(0, Math.min(45, Number(mapped.minute) || 0));
  const mappedBoots = mappedOwned
    .map((id) => itemById(id))
    .find((item) => item && item.boots);
  if (mappedBoots) live.roleQuestBoots = Number(mappedBoots.id);
  const questDone = live.mode === "classic" && live.role === "BOT" && live.minute >= 16;
  if (
    questDone &&
    !mappedBoots &&
    live.roleQuestBoots &&
    itemById(live.roleQuestBoots) &&
    !mappedOwned.includes(Number(live.roleQuestBoots))
  ) {
    // El cliente deja de exponer el slot 8 al completar la role quest. Si
    // vimos las botas antes, las mantenemos en la septima ranura.
    mappedOwned.push(Number(live.roleQuestBoots));
  }
  if (!questDone && !mappedBoots) live.roleQuestBoots = null;
  const cap = slotCapFor(live.mode, live.role, live.minute, mappedOwned.length);
  live.owned = mappedOwned.slice(0, cap);
  live.gold = Math.max(0, Math.min(100000, Number(mapped.gold) || 0));
  const scores = mapped.scores || {};
  live.game = {
    kills: Number(mapped.kills ?? scores.kills) || 0,
    deaths: Number(mapped.deaths ?? scores.deaths) || 0,
    assists: Number(mapped.assists ?? scores.assists) || 0,
    cs: Number(mapped.cs ?? scores.cs) || 0,
    level: Number(mapped.level) || 1,
    gameTime: Number(mapped.gameTime) || 0,
    teamKills: Number(mapped.teamKills) || 0,
    enemyKills: Number(mapped.enemyKills) || 0,
    teamCs: Number(mapped.teamCs) || 0,
    enemyCs: Number(mapped.enemyCs) || 0,
    teamLevel: Number(mapped.teamLevel) || 0,
    enemyLevel: Number(mapped.enemyLevel) || 0,
    enemyFed: Array.isArray(mapped.enemyFed) ? mapped.enemyFed.filter(Boolean) : [],
    enemyBuilds: Array.isArray(mapped.enemyBuilds) ? mapped.enemyBuilds : [],
    gameMode: mapped.gameMode || "",
    mapName: mapped.mapName || "",
    queueId: mapped.queueId ?? null,
  };

  renderAll();
  const source = $("#live-source");
  if (source) {
    const modeText = mode === "classic" ? "" : ` · ${modeLabel(mode)}`;
    source.textContent = `Cliente local${modeText} · actualizando…`;
    source.classList.add("live");
  }
  await refresh(true);
  return true;
}
