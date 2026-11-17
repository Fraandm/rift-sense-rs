/* Vista de datos: resumen del dataset y tabla del meta. */

import { api } from "./api.js";
import { $, avatarHTML, emptyState, esc, num, pct, rolePills } from "./ui.js";
import { meta, normalize, ROLE_SHORT, roleLabel } from "./state.js";

let loaded = false;
let rows = [];
let role = "ALL";
let query = "";
let sort = { key: "presence", dir: -1 };
let visible = 40;

const SORTS = {
  presence: (row) => row.presence || 0,
  winrate: (row) => row.winrate || 0,
  pickRate: (row) => row.pickRate || 0,
  banRate: (row) => row.banRate || 0,
  games: (row) => row.games || 0,
};

function renderMetrics(data) {
  const host = $("#data-metrics");
  if (!host) return;
  const s = data.summary || {};
  const cov = data.coverage || {};
  const range = s.dateRange || [];
  const ddr = s.ddragonVersion || meta.patch;
  host.innerHTML = `
    <div class="metric"><span>Partidas pro</span><strong>${num(s.games)}</strong><small>${esc(range.join(" → "))}</small></div>
    <div class="metric teal"><span>Filas de jugador</span><strong>${num(s.players)}</strong><small>una por build registrada</small></div>
    <div class="metric blue"><span>Torneos</span><strong>${num(s.tournaments)}</strong><small>ligas y circuitos</small></div>
    <div class="metric plain"><span>Campeones con datos</span><strong>${num(cov.championsWithData)} / ${num(cov.championPool)}</strong><small>mapeados a Data Dragon</small></div>
    <div class="metric plain"><span>Objetos en el vocabulario</span><strong>${num(cov.itemPool)}</strong><small>${num(cov.itemsWithData)} con compras</small></div>
    <div class="metric plain"><span>Versión estática</span><strong>${esc(ddr || "—")}</strong><small>parche de Data Dragon</small></div>
  `;
  const patch = $("#data-patch");
  if (patch) patch.textContent = `Parche ${ddr || "—"}`;
}

function renderCoverage(data) {
  const host = $("#data-coverage");
  if (!host) return;
  const summary = data.summary || {};
  const coverage = summary.coverage || {};
  const cov = data.coverage || {};
  const champPct = coverage.champions != null ? coverage.champions : 0;
  const itemPct = coverage.items != null ? coverage.items : 0;
  const poolPct = cov.championPool ? cov.championsWithData / cov.championPool : 0;
  const rows = [
    ["Campeones resueltos", champPct, `${pct(champPct, 2)}`],
    ["Objetos resueltos", itemPct, `${pct(itemPct, 2)}`],
    ["Campeones con muestra", poolPct, `${num(cov.championsWithData)} / ${num(cov.championPool)}`],
    ["Torneos distintos", 1, num(summary.tournaments)],
    ["Partidas válidas", 1, num(summary.games)],
  ];
  host.innerHTML = rows
    .map(
      ([label, value, text]) => `<div>
        <div class="cov-row"><strong>${label}</strong><span>${text}</span></div>
        <div class="cov-bar ${label.includes("Campeones") ? "gold" : ""}"><i style="width:${Math.max(
        2,
        Math.min(100, value * 100)
      )}%"></i></div>
      </div>`
    )
    .join("");
}

function renderTournaments(data) {
  const host = $("#data-tournaments");
  if (!host) return;
  const list = data.summary?.topTournaments || [];
  host.innerHTML = list.length
    ? list
        .map(
          (t) =>
            `<li><span title="${esc(t.name)}">${esc(t.name)}</span><b>${num(t.games)}</b></li>`
        )
        .join("")
    : `<li><span>Sin datos de torneos.</span></li>`;
}

function renderPipeline(data) {
  const host = $("#data-pipeline");
  if (!host) return;
  host.innerHTML = `
    <span>1 · Descargar partidas profesionales (reanudable)</span>
    <code>python -m lolanalyzer.scraper.leaguepedia --since 2024-01-01</code>
    <span>2 · Construir dataset, vocabulario y priors</span>
    <code>python -m lolanalyzer.data.build_dataset</code>
    <span>3 · Entrenar las redes</span>
    <code>python -m lolanalyzer.models.train_draft</code>
    <code>python -m lolanalyzer.models.train_build</code>
    <span>4 · Servir la interfaz y la API</span>
    <code>python -m lolanalyzer.api.server</code>
    <span>Fuente actual: ${esc(data.summary?.source || "Leaguepedia + Data Dragon")}</span>
  `;
}

function renderRoleFilter() {
  const host = $("#meta-role-filter");
  if (!host) return;
  const roles = ["ALL", ...meta.roles];
  host.innerHTML = roles
    .map(
      (r) =>
        `<button type="button" class="chip ${role === r ? "active" : ""}" data-meta-role="${r}">${
          r === "ALL" ? "Todos" : roleLabel(r)
        }</button>`
    )
    .join("");
}

function filtered() {
  const q = normalize(query);
  let list = rows.filter((row) => {
    if (role !== "ALL" && !(row.roles?.[role]?.n > 0)) return false;
    if (q && !normalize(row.name).includes(q)) return false;
    return true;
  });
  const keyFn = SORTS[sort.key] || SORTS.presence;
  list.sort((a, b) => (keyFn(a) - keyFn(b)) * sort.dir || a.name.localeCompare(b.name));
  return list;
}

function renderTable() {
  const tbody = $("#meta-table tbody");
  if (!tbody) return;
  const list = filtered();
  const slice = list.slice(0, visible);
  tbody.innerHTML = slice.length
    ? slice
        .map((row) => {
          const wrColor = row.winrate >= 0.52 ? "var(--teal)" : row.winrate <= 0.48 ? "var(--red)" : "var(--muted)";
          return `<tr>
            <td><div class="champ-cell">${avatarHTML(row)}<strong>${esc(row.name)}</strong></div></td>
            <td class="hide-sm">${rolePills(row.roles, 2)}</td>
            <td>${pct(row.pickRate, 2)}</td>
            <td>${pct(row.banRate, 2)}</td>
            <td style="color:${wrColor}">${pct(row.winrate, 1)}</td>
            <td><div class="bar-cell"><div class="track"><i class="fill" style="width:${Math.min(
              100,
              (row.presence || 0) * 100
            )}%"></i></div><span>${pct(row.presence, 0)}</span></div></td>
          </tr>`;
        })
        .join("")
    : `<tr><td colspan="6">${emptyState("i-search", "Sin coincidencias", "Prueba otro nombre o rol.")}</td></tr>`;
  const more = $("#meta-more");
  if (more) {
    more.classList.toggle("hidden", list.length <= visible);
    more.textContent = `Mostrar más (${Math.max(0, list.length - visible)})`;
  }
  const buttons = document.querySelectorAll("#meta-table th button[data-sort]");
  buttons.forEach((button) => {
    const active = button.dataset.sort === sort.key;
    button.textContent = `${button.dataset.label}${active ? (sort.dir < 0 ? " ↓" : " ↑") : ""}`;
  });
}

function render(data) {
  rows = data.champions || [];
  renderMetrics(data);
  renderCoverage(data);
  renderTournaments(data);
  renderPipeline(data);
  renderRoleFilter();
  renderTable();
}

export async function load() {
  if (loaded) return;
  loaded = true;
  try {
    const data = await api.get("/api/dataset");
    render(data);
  } catch (err) {
    loaded = false;
    const host = $("#data-metrics");
    if (host) host.innerHTML = emptyState("i-info", "No se pudo cargar el dataset", err.message);
  }
}

export function initEvents() {
  $("#meta-search")?.addEventListener("input", (event) => {
    query = event.target.value;
    renderTable();
  });

  $("#meta-role-filter")?.addEventListener("click", (event) => {
    const button = event.target.closest("[data-meta-role]");
    if (!button) return;
    role = button.dataset.metaRole;
    visible = 40;
    renderRoleFilter();
    renderTable();
  });

  $("#meta-table thead")?.addEventListener("click", (event) => {
    const button = event.target.closest("button[data-sort]");
    if (!button) return;
    const key = button.dataset.sort;
    if (sort.key === key) sort.dir *= -1;
    else sort = { key, dir: -1 };
    renderTable();
  });

  $("#meta-more")?.addEventListener("click", () => {
    visible += 40;
    renderTable();
  });

  document.querySelectorAll("#meta-table th button[data-sort]").forEach((button) => {
    button.dataset.label = button.textContent;
  });
}
