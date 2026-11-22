/* Utilidades de interfaz: formato, avatares, toasts, pickers y gauge. */

import {
  meta,
  championByName,
  iconUrl,
  normalize,
  roleLabel,
  ROLE_SHORT,
} from "./state.js";

export const $ = (sel, root = document) => root.querySelector(sel);
export const $$ = (sel, root = document) => [...root.querySelectorAll(sel)];

// La CSP no permite `onerror` inline. Centralizamos el fallback de imagenes
// rotas para no dejar iconos vacios ni errores en consola.
document.addEventListener(
  "error",
  (event) => {
    const image = event.target;
    if (image instanceof HTMLImageElement && image.dataset.fallback === "hide") {
      image.remove();
    }
  },
  true
);

export function esc(value) {
  return String(value ?? "")
    .replaceAll("&", "&amp;")
    .replaceAll("<", "&lt;")
    .replaceAll(">", "&gt;")
    .replaceAll('"', "&quot;")
    .replaceAll("'", "&#39;");
}

export function pct(value, decimals = 1) {
  if (value == null || Number.isNaN(Number(value))) return "—";
  return `${(Number(value) * 100).toFixed(decimals)}%`;
}

export function pp(value, decimals = 1) {
  if (value == null || Number.isNaN(Number(value))) return "—";
  const n = Number(value) * 100;
  return `${n >= 0 ? "+" : ""}${n.toFixed(decimals)} pp`;
}

export function num(value) {
  return new Intl.NumberFormat("es-ES").format(Number(value) || 0);
}

export function debounce(fn, wait = 250) {
  let timer;
  const wrapped = (...args) => {
    clearTimeout(timer);
    timer = setTimeout(() => fn(...args), wait);
  };
  wrapped.cancel = () => clearTimeout(timer);
  return wrapped;
}

export function avatarHTML(entity, size = "", kind = "champion") {
  if (!entity) return `<span class="avatar ${size}" data-initial="?" aria-hidden="true"></span>`;
  const file = entity.icon || (kind === "item" ? `${entity.id}.png` : "");
  const url = iconUrl(kind, file);
  const initial = esc(String(entity.name || "?").slice(0, 2).toUpperCase());
  return `<span class="avatar ${size}" data-initial="${initial}" aria-hidden="true">
    ${url ? `<img src="${esc(url)}" alt="" loading="lazy" decoding="async" data-fallback="hide">` : ""}
  </span>`;
}

export function clearToasts() {
  const host = $("#toasts");
  if (!host) return;
  host.replaceChildren();
}

export function toast(message, type = "info", ms = 3600) {
  const host = $("#toasts");
  if (!host) return;
  const duplicate = [...host.children].some(
    (child) => child.textContent === message && !child.classList.contains("out")
  );
  if (duplicate) return;
  const el = document.createElement("div");
  el.className = `toast ${type}`;
  el.textContent = message;
  host.append(el);
  while (host.children.length > 2) host.firstElementChild.remove();
  setTimeout(() => {
    el.classList.add("out");
    setTimeout(() => el.remove(), 220);
  }, ms);
}

export function setModeBadge(el, mode) {
  if (!el) return;
  el.classList.toggle("priors", mode !== "model");
  el.textContent = mode === "model" ? "Modelo" : "Datos pro";
}

export function skeletonList(container, count = 5, className = "skeleton-line") {
  if (!container) return;
  container.innerHTML = Array.from({ length: count })
    .map(() => `<div class="skeleton ${className}"></div>`)
    .join("");
}

export function emptyState(icon, title, text = "") {
  return `<div class="empty-state">
    <svg class="icon"><use href="#${icon}"/></svg>
    <strong>${esc(title)}</strong>
    ${text ? `<span>${esc(text)}</span>` : ""}
  </div>`;
}

export function renderGauge(el, probability) {
  if (!el) return;
  const value = Math.max(0, Math.min(1, Number(probability) || 0));
  const circle = el.querySelector(".gauge-value");
  const label = el.querySelector(".gauge-label strong");
  const C = 2 * Math.PI * 52;
  if (circle) {
    circle.style.strokeDasharray = String(C);
    circle.style.strokeDashoffset = String(C * (1 - value));
  }
  if (label) label.textContent = probability == null ? "—" : pct(value, 0);
  el.classList.toggle("good", value >= 0.53);
  el.classList.toggle("bad", value <= 0.47);
}

export function rolePills(roles = {}, limit = 3) {
  const entries = Object.entries(roles)
    .filter(([, value]) => (value?.n || 0) > 0)
    .sort((a, b) => (b[1]?.n || 0) - (a[1]?.n || 0))
    .slice(0, limit);
  return `<div class="role-pills">${entries
    .map(([r]) => `<span class="role-pill has">${ROLE_SHORT[r] || r}</span>`)
    .join("")}</div>`;
}

/* ------------------------------------------------------------------ */
/* Champion picker                                                     */
/* ------------------------------------------------------------------ */
const champDialog = document.getElementById("champ-picker");
const champGrid = document.getElementById("champ-grid");
const champSearch = document.getElementById("champ-search");
const champRoleFilter = document.getElementById("champ-role-filter");
const champHint = document.getElementById("champ-picker-hint");
const champEmpty = document.getElementById("champ-empty");
const champTitle = document.getElementById("champ-picker-title");
const champKicker = document.getElementById("champ-picker-kicker");

let champResolve = null;
let champState = { role: "MID", query: "", exclude: new Set(), title: "" };

function renderChampFilters() {
  const roles = ["ALL", ...meta.roles];
  champRoleFilter.innerHTML = roles
    .map(
      (r) =>
        `<button type="button" class="chip ${champState.role === r ? "active" : ""}" data-role="${r}">${
          r === "ALL" ? "Todos" : roleLabel(r)
        }</button>`
    )
    .join("");
}

function champMatches(champ) {
  const q = normalize(champState.query);
  if (q && !normalize(champ.name).includes(q) && !champ.tags?.some((t) => normalize(t).includes(q))) {
    return false;
  }
  if (champState.role !== "ALL" && !(champ.roles?.[champState.role]?.n > 0)) return false;
  return true;
}

function renderChampGrid() {
  const role = champState.role;
  let list = meta.champions.filter(champMatches);
  list.sort((a, b) => {
    if (role !== "ALL") {
      const byRole = (b.roles?.[role]?.n || 0) - (a.roles?.[role]?.n || 0);
      if (byRole) return byRole;
    }
    return (b.presence || 0) - (a.presence || 0) || a.name.localeCompare(b.name);
  });
  champGrid.innerHTML = list
    .map((champ) => {
      const blocked = champState.exclude.has(normalize(champ.name));
      const roleData = role !== "ALL" ? champ.roles?.[role] : null;
      const extra = blocked
        ? "No disponible"
        : roleData
        ? `${roleData.n} partidas · ${pct(roleData.winrate, 0)}`
        : `${pct(champ.presence, 0)} presencia`;
      return `<button type="button" class="champ-card ${blocked ? "disabled" : ""}" data-champ="${esc(
        champ.name
      )}" ${blocked ? "disabled" : ""}>
        ${avatarHTML(champ)}
        <strong>${esc(champ.name)}</strong>
        <small>${esc(extra)}</small>
      </button>`;
    })
    .join("");
  champEmpty.classList.toggle("hidden", list.length > 0);
  champHint.textContent = list.length
    ? `${list.length} campeones · ordenados por ${role === "ALL" ? "presencia" : "partidas en el rol"}`
    : "Prueba con otro nombre o cambia el filtro de rol.";
}

export function openChampionPicker({ role = "MID", exclude = [], title = "Elegir campeón", kicker = "Campeones" } = {}) {
  if (!champDialog) return Promise.resolve(null);
  champState = {
    role: role || "ALL",
    query: "",
    exclude: new Set(exclude.map((n) => normalize(n))),
    title,
  };
  champTitle.textContent = title;
  champKicker.textContent = kicker;
  champSearch.value = "";
  renderChampFilters();
  renderChampGrid();
  champDialog.showModal();
  setTimeout(() => champSearch.focus(), 40);
  return new Promise((resolve) => {
    champResolve = resolve;
  });
}

champSearch?.addEventListener("input", () => {
  champState.query = champSearch.value;
  renderChampGrid();
});

champRoleFilter?.addEventListener("click", (event) => {
  const button = event.target.closest("button[data-role]");
  if (!button) return;
  champState.role = button.dataset.role;
  renderChampFilters();
  renderChampGrid();
});

champGrid?.addEventListener("click", (event) => {
  const card = event.target.closest(".champ-card");
  if (!card || card.disabled) return;
  const name = card.dataset.champ;
  champResolve?.(name);
  champResolve = null;
  champDialog.close();
});

champSearch?.addEventListener("keydown", (event) => {
  if (event.key !== "Enter") return;
  event.preventDefault();
  const first = champGrid?.querySelector(".champ-card:not(.disabled)");
  if (first) first.click();
});

champDialog?.addEventListener("close", () => {
  if (champResolve) {
    champResolve(null);
    champResolve = null;
  }
});

champDialog?.addEventListener("click", (event) => {
  if (event.target === champDialog) champDialog.close();
});

/* ------------------------------------------------------------------ */
/* Item picker                                                          */
/* ------------------------------------------------------------------ */
const itemDialog = document.getElementById("item-picker");
const itemGrid = document.getElementById("item-grid");
const itemSearch = document.getElementById("item-search");
const itemTagFilter = document.getElementById("item-tag-filter");
const itemHint = document.getElementById("item-picker-hint");
const itemEmpty = document.getElementById("item-empty");
const itemTitle = document.getElementById("item-picker-title");

const ITEM_TAGS = [
  { id: "ALL", label: "Todos" },
  { id: "DAMAGE", label: "Daño" },
  { id: "AP", label: "AP" },
  { id: "DEFENSE", label: "Defensa" },
  { id: "BOOTS", label: "Botas" },
  { id: "SUPPORT", label: "Soporte" },
];

let itemResolve = null;
let itemState = { tag: "ALL", query: "", exclude: new Set() };

function renderItemFilters() {
  itemTagFilter.innerHTML = ITEM_TAGS.map(
    (t) =>
      `<button type="button" class="chip ${itemState.tag === t.id ? "active" : ""}" data-tag="${t.id}">${t.label}</button>`
  ).join("");
}

function itemMatches(item) {
  const q = normalize(itemState.query);
  if (q && !normalize(item.name).includes(q) && !item.tags?.some((t) => normalize(t).includes(q))) return false;
  const tags = new Set(item.tags || []);
  switch (itemState.tag) {
    case "DAMAGE":
      return tags.has("Damage") || tags.has("CriticalStrike") || tags.has("AttackSpeed");
    case "AP":
      return tags.has("SpellDamage");
    case "DEFENSE":
      return tags.has("Armor") || tags.has("SpellBlock") || tags.has("Health");
    case "BOOTS":
      return Boolean(item.boots);
    case "SUPPORT":
      return tags.has("GoldPer") || tags.has("ManaRegen") || tags.has("HealthRegen");
    default:
      return true;
  }
}

function renderItemGrid() {
  const list = meta.itemsByGold.filter(itemMatches);
  itemGrid.innerHTML = list
    .map((item) => {
      const blocked = itemState.exclude.has(Number(item.id));
      return `<button type="button" class="item-card ${blocked ? "disabled" : ""}" data-item="${item.id}" ${
        blocked ? "disabled" : ""
      }>
        <span class="item-top">
          ${avatarHTML(item, "", "item")}
          <span><strong>${esc(item.name)}</strong><br><small>${num(item.gold)} oro</small></span>
        </span>
        <span class="item-tags">${(item.tags || []).slice(0, 3).map((t) => `<span>${esc(t)}</span>`).join("")}</span>
      </button>`;
    })
    .join("");
  itemEmpty.classList.toggle("hidden", list.length > 0);
  itemHint.textContent = list.length ? `${list.length} objetos comprables` : "Sin resultados para ese filtro.";
}

export function openItemPicker({ exclude = [], title = "Añadir objeto" } = {}) {
  if (!itemDialog) return Promise.resolve(null);
  itemState = { tag: "ALL", query: "", exclude: new Set(exclude.map(Number)) };
  itemTitle.textContent = title;
  itemSearch.value = "";
  renderItemFilters();
  renderItemGrid();
  itemDialog.showModal();
  setTimeout(() => itemSearch.focus(), 40);
  return new Promise((resolve) => {
    itemResolve = resolve;
  });
}

itemSearch?.addEventListener("input", () => {
  itemState.query = itemSearch.value;
  renderItemGrid();
});

itemTagFilter?.addEventListener("click", (event) => {
  const button = event.target.closest("button[data-tag]");
  if (!button) return;
  itemState.tag = button.dataset.tag;
  renderItemFilters();
  renderItemGrid();
});

itemGrid?.addEventListener("click", (event) => {
  const card = event.target.closest(".item-card");
  if (!card || card.disabled) return;
  const id = Number(card.dataset.item);
  itemResolve?.(id);
  itemResolve = null;
  itemDialog.close();
});

itemSearch?.addEventListener("keydown", (event) => {
  if (event.key !== "Enter") return;
  event.preventDefault();
  const first = itemGrid?.querySelector(".item-card:not(.disabled)");
  if (first) first.click();
});

itemDialog?.addEventListener("close", () => {
  if (itemResolve) {
    itemResolve(null);
    itemResolve = null;
  }
});

itemDialog?.addEventListener("click", (event) => {
  if (event.target === itemDialog) itemDialog.close();
});

document.addEventListener("click", (event) => {
  const close = event.target.closest("[data-close]");
  if (!close) return;
  close.closest("dialog")?.close();
});

/* Resuelve nombres de campeón para slots que solo tienen el string. */
export function championForSlot(name) {
  return name ? championByName(name) : null;
}
