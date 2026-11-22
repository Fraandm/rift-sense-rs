/* Resumen post-partida: build real, compras y seguimiento. */

import { championByName, itemById } from "./state.js";
import { api } from "./api.js";
import {
  $,
  avatarHTML,
  emptyState,
  esc,
  num,
  pct,
  toast,
} from "./ui.js";
import {
  clearSessions,
  importPostgame,
  loadSessions,
  setManualResult,
} from "./session.js";

const MODE_LABELS = {
  classic: "Grieta",
  aram: "ARAM",
  mayhem: "Mayhem",
  arena: "Arena",
};

let bound = false;

function modeLabel(mode) {
  return MODE_LABELS[mode] || "Grieta";
}

function resultBadge(session) {
  if (session.result === "win") return `<span class="badge teal">Victoria</span>`;
  if (session.result === "loss") return `<span class="badge red">Derrota</span>`;
  return `<span class="badge">Sin resultado</span>`;
}

function itemsFor(ids) {
  return (ids || []).map((id) => {
    const item = itemById(id);
    return item || { id, name: `Objeto ${id}`, icon: `${id}.png`, gold: 0 };
  });
}

function purchaseStatus(purchase) {
  if (purchase.initial) return { label: "Inventario inicial", tone: "" };
  if (purchase.followed) return { label: "Siguió la recomendación", tone: "teal" };
  if (purchase.planned) return { label: "Estaba en el plan", tone: "gold" };
  return { label: "Fuera del plan", tone: "bad" };
}

function renderMetrics(session) {
  const counted = (session.purchases || []).filter((purchase) => !purchase.initial);
  const total = counted.length;
  const followed = Number(session.followed) || 0;
  const planned = Number(session.planned) || 0;
  const formatDuration = (seconds) => {
    const value = Math.round(Number(seconds) || 0);
    if (!value) return "—";
    return `${Math.floor(value / 60)}:${String(value % 60).padStart(2, "0")}`;
  };
  return `<div class="summary-metrics">
    <div><span>KDA</span><strong>${session.kda?.kills || 0}/${session.kda?.deaths || 0}/${
      session.kda?.assists || 0
    }</strong></div>
    <div><span>Duración</span><strong>${formatDuration(session.durationSeconds)}</strong></div>
    <div><span>Acierto de compra</span><strong>${total ? pct(followed / total, 0) : "—"}</strong></div>
    <div><span>Dentro del plan</span><strong>${total ? pct((followed + planned) / total, 0) : "—"}</strong></div>
  </div>`;
}

function renderBuild(session) {
  const items = itemsFor(session.finalItems);
  if (!items.length) {
    return `<p class="summary-empty">Sin inventario final registrado.</p>`;
  }
  return `<div class="summary-build">${items
    .map(
      (item) => `<span class="summary-build-item" title="${esc(item.name)}">
        ${avatarHTML(item, "sm", "item")}
        <span>${esc(item.name)}</span>
      </span>`
    )
    .join("")}</div>`;
}

function renderPurchases(session) {
  const purchases = session.purchases || [];
  if (!purchases.length) {
    return `<p class="summary-empty">${
      session.source === "match-history"
        ? "El historial del cliente no incluye la secuencia de compras."
        : "Sin compras registradas."
    }</p>`;
  }
  return `<div class="summary-purchases">${purchases
    .map((purchase) => {
      const item = itemById(purchase.id) || {
        id: purchase.id,
        name: purchase.name,
        icon: purchase.icon,
      };
      const status = purchaseStatus(purchase);
      return `<div class="summary-purchase">
        ${avatarHTML(item, "sm", "item")}
        <span class="summary-purchase-name">${esc(purchase.name)}</span>
        <span class="summary-purchase-minute">${Math.round(Number(purchase.minute) || 0)}′</span>
        <span class="reason ${status.tone}">${status.label}</span>
      </div>`;
    })
    .join("")}</div>`;
}

function renderManualCorrections(session) {
  const team = session.mode === "classic" ? [] : session.allies || [];
  if (
    !session.augments?.length &&
    !session.augmentIds?.length &&
    !session.anvils?.length &&
    !team.length
  )
    return "";
  const bits = [];
  const allies = session.mode === "arena" ? team.slice(0, 3) : team;
  if (allies.length) bits.push(`${session.mode === "arena" ? "Trío" : "Equipo"}: ${allies.join(", ")}`);
  if (session.augments?.length) {
    bits.push(`Augments: ${session.augments.join(", ")}`);
  } else if (session.augmentIds?.length) {
    bits.push(`Augments detectados (ID): ${session.augmentIds.join(", ")}`);
  }
  if (session.anvils?.length) {
    const labels = { stats: "Stats", legendary: "Legendario", prismatic: "Prismático" };
    bits.push(`Anviles: ${session.anvils.map((type) => labels[type] || type).join(", ")}`);
  }
  const manual = session.augments?.length || session.anvils?.length;
  const title = manual
    ? "Correcciones manuales"
    : session.augmentIds?.length
      ? "Detectado por el cliente"
      : "Composición";
  return `<div class="summary-manual"><strong>${title}</strong>${bits
    .map((bit) => `<span>${esc(bit)}</span>`)
    .join("")}</div>`;
}

function renderSession(session, latest) {
  const champ = championByName(session.champion);
  const date = session.startedAt
    ? new Date(session.startedAt).toLocaleString("es-ES", {
        day: "2-digit",
        month: "2-digit",
        hour: "2-digit",
        minute: "2-digit",
      })
    : "Sin fecha";
  const manualResult =
    session.result == null
      ? `<div class="summary-result-actions">
          <span>Resultado no disponible:</span>
          <button type="button" class="btn tiny" data-session-result="win" data-session-id="${esc(
            session.id
          )}">Victoria</button>
          <button type="button" class="btn tiny ghost" data-session-result="loss" data-session-id="${esc(
            session.id
          )}">Derrota</button>
        </div>`
      : "";
  const source = session.source === "match-history" ? "Historial del cliente" : "Seguimiento en vivo";
  return `<article class="card summary-card ${latest ? "latest" : ""}">
    <header class="summary-head">
      <div class="summary-who">
        ${champ ? avatarHTML(champ, "lg") : `<span class="avatar lg" data-initial="?" aria-hidden="true"></span>`}
        <div>
          <p class="eyebrow">${esc(modeLabel(session.mode))} · ${esc(date)} · ${esc(source)}</p>
          <h2>${esc(session.champion || "Partida")}</h2>
        </div>
      </div>
      ${resultBadge(session)}
    </header>
    ${renderMetrics(session)}
    <div class="summary-columns">
      <section><h3>Build real</h3>${renderBuild(session)}</section>
      <section><h3>Objetos comprados</h3>${renderPurchases(session)}</section>
    </div>
    ${renderManualCorrections(session)}
    ${manualResult}
  </article>`;
}

function renderList() {
  const host = $("#summary-content");
  if (!host) return;
  const sessions = loadSessions();
  const clear = $("#summary-clear");
  if (clear) clear.disabled = !sessions.length;
  if (!sessions.length) {
    host.innerHTML = emptyState(
      "i-trophy",
      "Todavía no hay partidas",
      "Juega con Auto activo o lee una partida desde Cliente. Al terminar se guardará la build real, las compras y el resultado."
    );
    return;
  }
  host.innerHTML = `<div class="summary-list">${sessions
    .map((session, index) => renderSession(session, index === 0))
    .join("")}</div>`;
}

export async function refresh() {
  renderList();
  const postgame = await api
    .get("/api/client/postgame")
    .catch(() => ({ available: false }));
  if (importPostgame(postgame)) renderList();
}

export function init() {
  if (bound) return;
  bound = true;
  refresh();

  $("#summary-clear")?.addEventListener("click", () => {
    if (!loadSessions().length) return;
    if (!window.confirm("¿Borrar todo el historial de partidas?")) return;
    clearSessions();
    refresh();
    toast("Historial borrado.", "info");
  });

  $("#summary-content")?.addEventListener("click", (event) => {
    const button = event.target.closest("[data-session-result]");
    if (!button) return;
    const id = button.dataset.sessionId;
    const win = button.dataset.sessionResult === "win";
    if (setManualResult(id, win)) {
      refresh();
      toast("Resultado guardado.", "success");
    }
  });
}
