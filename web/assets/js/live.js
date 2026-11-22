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

/* ------------------------------------------------------------------ */
/* Estado                                                              */
/* ------------------------------------------------------------------ */
function syncChampionSlot() {
  const idx = roleIndex(live.role);
  live.allies = live.allies.map((name) => (name === live.champion ? null : name));
  if (live.champion) live.allies[idx] = live.champion;
}

function setChampion(name) {
  if (!name) return;
  const idx = roleIndex(live.role);
  live.allies = live.allies.map((n) => (n === name ? null : n));
  live.champion = name;
  live.allies[idx] = name;
  renderAll();
  refresh(true);
}

function changeRole(role) {
  if (role === live.role) return;
  const oldIdx = roleIndex(live.role);
  const newIdx = roleIndex(role);
  const oldChamp = live.champion;
  if (oldChamp && live.allies[oldIdx] === oldChamp) live.allies[oldIdx] = null;
  live.role = role;
  if (oldChamp) live.allies[newIdx] = oldChamp;
  renderAll();
  refresh(true);
}

function addItem(id) {
  if (live.owned.length >= 6) {
    toast("El inventario ya tiene 6 objetos.", "error");
    return;
  }
  if (live.owned.includes(Number(id))) return;
  const item = itemById(id);
  if (item?.boots && live.owned.some((ownedId) => itemById(ownedId)?.boots)) {
    toast("Ya llevas unas botas. Vende las actuales antes de comprar otras.", "error");
    return;
  }
  live.owned.push(Number(id));
  renderInventory();
  refresh(true);
}

function removeItem(index) {
  live.owned.splice(index, 1);
  renderInventory();
  refresh(true);
}

function buyItem(id) {
  const item = itemById(id);
  if (!item) return;
  if (live.owned.length >= 6) {
    toast("Inventario completo.", "error");
    return;
  }
  if (live.gold < item.gold) {
    toast(`Te faltan ${num(item.gold - live.gold)} de oro para ${item.name}.`, "error");
    return;
  }
  live.gold = Math.max(0, live.gold - item.gold);
  live.owned.push(Number(id));
  const input = $("#live-gold");
  if (input) input.value = String(live.gold);
  renderInventory();
  refresh(true);
}

/* ------------------------------------------------------------------ */
/* Render                                                              */
/* ------------------------------------------------------------------ */
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
    kicker.textContent = champ ? `${champ.name} · ${roleLabel(live.role)}` : "Sin campeón";
  }
}

function renderMiniSlots() {
  for (const [team, selector] of [
    ["allies", "#live-allies"],
    ["enemies", "#live-enemies"],
  ]) {
    const host = $(selector);
    if (!host) continue;
    host.innerHTML = meta.roles
      .map((role, index) => {
        const name = live[team][index];
        const champ = championByName(name);
        const mine = team === "allies" && index === roleIndex(live.role);
        return `<button type="button" class="mini-slot ${champ ? "filled" : ""} ${mine ? "mine" : ""}" data-mini-team="${team}" data-index="${index}" title="${
          champ ? `Cambiar ${champ.name}` : `Elegir ${roleLabel(role)}`
        }">
          ${champ ? avatarHTML(champ) : `<svg class="icon"><use href="#i-plus"/></svg>`}
          <span class="mini-role">${roleLabel(role).slice(0, 3).toUpperCase()}</span>
        </button>`;
      })
      .join("");
  }
  const allyCount = live.allies.filter(Boolean).length;
  const enemyCount = live.enemies.filter(Boolean).length;
  const allyLabel = $("#live-ally-count");
  const enemyLabel = $("#live-enemy-count");
  if (allyLabel) allyLabel.textContent = `${allyCount}/5`;
  if (enemyLabel) enemyLabel.textContent = `${enemyCount}/5`;
}

function renderInventory() {
  const host = $("#live-inventory");
  if (!host) return;
  const slots = [];
  for (let i = 0; i < 6; i += 1) {
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
      slots.push(`<button type="button" class="inv-slot" data-inv-add="${i}" aria-label="Añadir objeto">
        <svg class="icon"><use href="#i-plus"/></svg>
        <span class="inv-name">Objeto ${i + 1}</span>
      </button>`);
    }
  }
  host.innerHTML = slots.join("");
  const count = $("#live-slot-count");
  if (count) count.textContent = `${live.owned.length} / 6`;
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
        ${c.icon ? `<img src="${esc(iconUrl("item", c.icon))}" alt="" loading="lazy" onerror="this.remove()">` : ""}
        ${esc(c.name)}
      </span>`
    )
    .join("")}</div>`;
}

function renderItemCard(item, isNext = false) {
  const disabled = !item.affordable || live.owned.length >= 6;
  const label = live.owned.length >= 6 ? "Lleno" : item.affordable ? "Comprar" : "Sin oro";
  const actionNote = item.affordable ? "disponible" : `faltan ${num(item.missingGold)}`;
  return `<article class="item-reco ${isNext ? "next-buy" : ""}">
    ${avatarHTML(item, "", "item")}
    <div class="item-reco-main">
      <div class="item-reco-title">
        <strong>${esc(item.name)}</strong>
        <span class="item-cost"><svg class="icon"><use href="#i-gold"/></svg>${num(item.gold)}</span>
        ${isNext ? `<span class="reason" style="color:var(--teal);border-color:rgba(10,200,185,.3)">Compra ahora</span>` : ""}
      </div>
      <div class="item-reasons">${itemReasons(item.reasons, [`Score ${Math.round(item.score)}`])}</div>
      ${componentsHTML(item.components)}
    </div>
    <div class="item-side">
      <strong>${Math.round(item.score)}</strong>
      <small>ajuste</small>
      <div class="score-bar"><i style="width:${Math.max(3, Math.min(100, item.score))}%"></i></div>
    </div>
    <div class="item-action">
      <button type="button" class="btn tiny" data-buy="${item.id}" ${disabled ? "disabled" : ""}>${label}</button>
      <small>${actionNote}</small>
    </div>
  </article>`;
}

function renderItems(build) {
  const host = $("#live-items");
  const subtitle = $("#live-reco-subtitle");
  const badge = $("#live-mode");
  if (!host) return;
  setModeBadge(badge, build?.mode);

  if (!live.champion) {
    host.innerHTML = emptyState("i-brain", "Elige tu campeón", "RiftSense necesita el campeón para buscar builds con contexto.");
    if (subtitle) subtitle.textContent = "Recomendaciones contextualizadas";
    return;
  }
  if (build?.mode === "complete") {
    host.innerHTML = emptyState("i-trophy", "Build completa", "Ya tienes 6 objetos. Reserva oro para el trinket y consumibles.");
    if (subtitle) subtitle.textContent = "Inventario lleno";
    return;
  }
  const items = build?.items || [];
  if (!items.length) {
    host.innerHTML = emptyState("i-info", "Sin recomendaciones", "Añade rivales o revisa el inventario.");
    if (subtitle) subtitle.textContent = "Sin candidatos";
    return;
  }
  if (subtitle) {
    const component = build?.nextComponent
      ? ` · componente: ${build.nextComponent.name}`
      : "";
    subtitle.textContent = `Slot ${build.slot} de 6 · ${items.length} candidatos · ${
      build.mode === "model" ? "modelo + datos" : "datos pro"
    }${component}`;
  }

  const nextId = build?.nextPurchase?.id;
  const cards = [];
  if (build?.nextPurchase && !items.some((item) => item.id === nextId)) {
    cards.push(renderItemCard(build.nextPurchase, true));
  }
  for (const item of items) cards.push(renderItemCard(item, item.id === nextId));
  host.innerHTML = cards.join("");
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

  const next = build?.nextPurchase;
  const component = build?.nextComponent;
  const top = build?.items?.[0];
  const nextName = next?.name || component?.name || "Ahorra oro";
  let nextDetail;
  if (next) {
    nextDetail = "Puedes comprarlo ahora";
  } else if (component) {
    nextDetail = `Componente de ${component.parent}`;
  } else if (top?.missingGold) {
    nextDetail = `Faltan ${num(top.missingGold)} para ${top.name}`;
  } else {
    nextDetail = "Guarda oro para el siguiente spike";
  }

  const notes = (analysis.notes || []).slice(0, 3).join(" · ");
  const extras = [];
  if (analysis.enemyFed?.length) extras.push(`Fuertes: ${analysis.enemyFed.join(", ")}`);
  if (analysis.healingThreat?.length) extras.push(`Curación: ${analysis.healingThreat.join(", ")}`);

  host.className = `live-coach ${analysis.state || ""}`;
  host.innerHTML = `
    <div class="coach-copy">
      <span class="eyebrow">Lectura de partida</span>
      <strong>${esc(analysis.label)}</strong>
      <div class="coach-metrics">
        <span>KDA ${analysis.kills}/${analysis.deaths}/${analysis.assists}</span>
        <span>${num(analysis.cs)} CS · ${analysis.csPerMin}/min</span>
        <span>Nivel ${analysis.level}</span>
        <span>Equipos ${analysis.teamKills}-${analysis.enemyKills}</span>
        <span>Niveles ${analysis.teamLevel}-${analysis.enemyLevel}</span>
      </div>
      <p>${esc(notes)}${extras.length ? ` · ${esc(extras.join(" · "))}` : ""}</p>
    </div>
    <div class="coach-next">
      <span class="eyebrow">Próxima compra</span>
      <strong>${esc(nextName)}</strong>
      <span>${esc(nextDetail)}</span>
    </div>`;
  host.classList.remove("hidden");
}

function renderVerdict(draft, build) {
  const champ = championByName(live.champion);
  const title = $("#live-verdict-title");
  const text = $("#live-verdict-text");
  const pills = $("#live-pills");
  const probability = draft?.winProbability ?? null;

  renderGauge($("#live-gauge"), probability);

  if (!champ) {
    if (title) title.textContent = "Configura tu partida";
    if (text) text.textContent = "Elige un campeón y añade rivales para recibir recomendaciones de build con contexto.";
    if (pills) pills.innerHTML = "";
    return;
  }

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
      `<span class="pill gold">${live.owned.length}/6 objetos</span>`,
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
    `</div><div class="plan-cost">Coste del plan: <b>${num(total)}</b> oro (sin contar lo ya comprado)</div>`;
}

function renderNotes(build, draft) {
  
  const host = $("#live-notes");
  if (!host) return;
  if (!live.champion) {
    host.innerHTML = emptyState("i-info", "Sin datos todavía", "Completa el estado para ver el perfil del rival y los spikes.");
    return;
  }
  const rows = [];
  const nearby = build?.powerSpike?.nearby || {};
  const entries = Object.entries(nearby)
    .map(([slot, value]) => [Number(slot), value])
    .sort((a, b) => a[0] - b[0]);
  if (entries.length) {
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
    rows.push(`<div class="note-box"><strong>Perfil del rival</strong>${
      profile.known < 5 ? "Faltan campeones enemigos, el cálculo es aproximado. " : ""
    }La composición rival es ${profile.label === "Mixta" ? "de daño mixto" : `principalmente ${profile.label}`} (${pct(
      profile.physical,
      0
    )} físico / ${pct(profile.magic, 0)} mágico).</div>`);
  }

  if (draft) {
    rows.push(`<div class="note-box ${draft.complete ? "good" : "warn"}"><strong>Draft ${
      draft.complete ? "completo" : "incompleto"
    }</strong>${pct(draft.winProbability, 1)} de victoria estimada. ${
      draft.complete ? "La estimación usa los 10 picks." : `Faltan ${10 - (draft.knownPicks || 0)} picks.`
    }</div>`);
  }

  if (state.draft.bans.length) {
    rows.push(`<div class="note-box"><strong>Bans del draft</strong>Se están usando los ${state.draft.bans.length} bans configurados en la pestaña Draft.</div>`);
  }

  const notes = build?.notes || [];
  if (notes.length) {
    rows.push(`<div class="note-box"><strong>Notas del motor</strong>${notes.map((n) => esc(n)).join("<br>")}</div>`);
  }

  rows.push(`<div class="note-box"><strong>Alcance</strong>Con Auto activo, RiftSense lee el estado de la partida cada 3 segundos (KDA, CS, niveles, equipo, oro e inventario) y adapta la siguiente compra. Sin partida en curso puedes introducir los datos manualmente. Nada de esto envía compras al juego.</div>`);
  host.innerHTML = rows.join("");
}

/* ------------------------------------------------------------------ */
/* Request                                                             */
/* ------------------------------------------------------------------ */
function payload() {
  return {
    champion: live.champion,
    role: live.role,
    side: live.side,
    allies: live.allies,
    enemies: live.enemies,
    owned: live.owned,
    gold: Number(live.gold) || 0,
    minute: Number(live.minute) || 0,
    game: live.game,
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
  renderRoles();
  renderSide();
  renderChampionSelect();
  renderMiniSlots();
  renderInventory();
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

  $("#live-roles")?.addEventListener("click", (event) => {
    const button = event.target.closest("[data-live-role]");
    if (button) changeRole(button.dataset.liveRole);
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
      role: live.role,
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
      const role = meta.roles[index];
      const current = live[team][index];
      if (team === "allies" && index === roleIndex(live.role)) {
        const name = await openChampionPicker({
          role: live.role,
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
        role,
        exclude,
        title: `${team === "allies" ? "Aliado" : "Rival"} · ${roleLabel(role)}`,
        kicker: "Composición",
      });
      if (!name) return;
      live[team][index] = name;
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

  $("#live-items")?.addEventListener("click", (event) => {
    const button = event.target.closest("[data-buy]");
    if (!button || button.disabled) return;
    buyItem(Number(button.dataset.buy));
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
    live.champion = "Ahri";
    live.role = "MID";
    live.side = "blue";
    live.allies = ["K'Sante", "Lee Sin", "Ahri", "Jinx", "Rakan"]
      .map((n) => (championByName(n) ? n : null));
    live.enemies = ["Gnar", "Vi", "Orianna", "Kai'Sa", "Nautilus"]
      .map((n) => (championByName(n) ? n : null));
    live.owned = ["Doran's Ring", "Sorcerer's Shoes"]
      .map((n) => itemByName(n)?.id)
      .filter((id) => id != null);
    live.gold = 1600;
    live.minute = 9;
    live.game = {
      kills: 4,
      deaths: 2,
      assists: 5,
      cs: 120,
      level: 10,
      gameTime: 540,
      teamKills: 10,
      enemyKills: 7,
      teamCs: 500,
      enemyCs: 420,
      teamLevel: 9.6,
      enemyLevel: 9.1,
      enemyFed: ["Vi"],
    };
    const source = $("#live-source");
    if (source) {
      source.textContent = "Fuente manual · ejemplo";
      source.classList.remove("live");
    }
    renderAll();
    refresh(true);
    toast("Partida de ejemplo cargada: Ahri mid, 9 minutos.", "success");
  });

  $("#live-reset")?.addEventListener("click", () => {
    live.champion = null;
    live.role = "MID";
    live.side = "blue";
    live.allies = [null, null, null, null, null];
    live.enemies = [null, null, null, null, null];
    live.owned = [];
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
export async function applyClientState(mapped) {
  if (!mapped?.available) return false;
  const champion = championByName(mapped.champion);
  if (!champion) return false;

  const pad = (list) =>
    Array.from({ length: 5 }, (_, index) => {
      const name = Array.isArray(list) ? list[index] : null;
      return name && championByName(name) ? name : null;
    });

  live.champion = champion.name;
  live.role = meta.roles.includes(mapped.role) ? mapped.role : live.role;
  live.side = mapped.side === "red" ? "red" : "blue";
  live.allies = pad(mapped.allies);
  live.enemies = pad(mapped.enemies);
  const ownIndex = roleIndex(live.role);
  if (!live.allies[ownIndex]) live.allies[ownIndex] = champion.name;

  live.owned = (Array.isArray(mapped.owned) ? mapped.owned : [])
    .filter((id) => itemById(id))
    .slice(0, 6);
  live.gold = Math.max(0, Math.min(100000, Number(mapped.gold) || 0));
  live.minute = Math.max(0, Math.min(45, Number(mapped.minute) || 0));
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
  };

  renderAll();
  const source = $("#live-source");
  if (source) {
    source.textContent = "Cliente local · actualizando…";
    source.classList.add("live");
  }
  await refresh(true);
  return true;
}
