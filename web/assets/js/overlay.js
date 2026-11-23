/* Overlay compacto de partida: solo la siguiente compra y los ajustes clave. */

const $ = (id) => document.getElementById(id);
const ROLE_LABEL = {
  TOP: "Top",
  JUNGLE: "Jungla",
  MID: "Mid",
  BOT: "Bot",
  SUPPORT: "Support",
};
const REFRESH_MS = 2600;

const overlay = $("overlay");
const art = $("art");
const itemIcon = $("item-icon");
const pulse = $("pulse");
const eyebrow = $("eyebrow");
const champ = $("champ");
const itemName = $("item-name");
const hint = $("hint");
const gold = $("gold");
const extras = $("extras");
const bootsChip = $("boots");
const situationalChip = $("situational");

let patch = "";
let busy = false;
let hasBuild = false;
let lastSignature = "";

function number(value) {
  const numeric = Number(value);
  return Number.isFinite(numeric) ? numeric : 0;
}

function formatGold(value) {
  return new Intl.NumberFormat("es-ES").format(Math.max(0, Math.round(number(value))));
}

function iconUrl(kind, file) {
  if (!file || !patch) return "";
  return `https://ddragon.leagueoflegends.com/cdn/${patch}/img/${kind}/${file}`;
}

function setImage(source, alt) {
  if (!source) {
    itemIcon.removeAttribute("src");
    itemIcon.alt = "";
    return;
  }
  itemIcon.src = source;
  itemIcon.alt = alt || "";
}

function setText(element, value) {
  element.textContent = value || "";
}

function setChip(element, label, text, warn = false) {
  element.replaceChildren();
  element.classList.toggle("warn", warn);
  if (!label || !text) return;
  const strong = document.createElement("b");
  strong.textContent = `${label}: `;
  element.append(strong, document.createTextNode(text));
}

function showExtras(show) {
  extras.classList.toggle("hidden", !show);
}

function waiting(title = "Esperando partida", detail = "Se activa al entrar en partida") {
  hasBuild = false;
  lastSignature = "";
  overlay.classList.add("waiting");
  overlay.classList.remove("complete");
  pulse.classList.remove("hidden");
  art.classList.add("empty");
  art.classList.remove("done");
  setImage("", "");
  setText(eyebrow, "RIFTSENSE");
  setText(champ, "");
  setText(itemName, title);
  setText(hint, detail);
  setText(gold, "—");
  setChip(bootsChip, "", "");
  setChip(situationalChip, "", "");
  showExtras(false);
}

function situationalLabel(note) {
  const text = String(note || "").toLowerCase();
  if (text.includes("antiheal")) return "Antiheal";
  if (text.includes("penetración de armadura")) return "AD";
  if (text.includes("penetración mágica")) return "AP";
  if (text.includes("escudo")) return "Escudos";
  if (text.includes("vida")) return "Vida";
  if (text.includes("autoataque")) return "Armadura";
  return "Ajuste";
}

function costText(item) {
  if (!item) return "";
  if (item.affordable) return "Listo para comprar";
  return `Faltan ${formatGold(item.missingGold)}`;
}

function renderComplete(build) {
  hasBuild = true;
  lastSignature = "complete";
  overlay.classList.remove("waiting");
  overlay.classList.add("complete");
  pulse.classList.add("hidden");
  art.classList.add("empty", "done");
  setImage("", "");
  setText(eyebrow, "BUILD COMPLETA");
  const champion = build?.champion?.name || "";
  setText(champ, champion);
  setText(itemName, "Sin compras pendientes");
  const slots = number(build?.slotCap) || number(build?.slot) || 6;
  setText(hint, `${slots} objetos · reserva oro para consumibles`);
  setText(gold, formatGold(build?.gold));
  setChip(bootsChip, "", "");
  setChip(situationalChip, "", "");
  showExtras(false);
}

function renderBuild(build) {
  const next = Array.isArray(build?.items) ? build.items[0] : null;
  if (build?.mode === "complete" || (!next && build?.fullInventory)) {
    renderComplete(build);
    return;
  }
  if (!next) {
    waiting("Sin compra disponible", "El motor no encontró una opción ahora mismo");
    return;
  }

  const signature = [
    next.id,
    Math.round(number(build?.gold)),
    Math.round(number(next?.missingGold)),
    build?.mode,
  ].join(":");
  hasBuild = true;
  overlay.classList.remove("waiting", "complete");
  art.classList.remove("empty", "done");
  pulse.classList.add("hidden");

  const champion = build?.champion?.name || build?.champion || "";
  const role = ROLE_LABEL[build?.role] || build?.role || "";
  setImage(iconUrl("item", next.icon), next.name);
  setText(eyebrow, next.upgrade ? "COMPLETAR OBJETIVO" : "SIGUIENTE COMPRA");
  setText(champ, [champion, role].filter(Boolean).join(" · "));
  setText(itemName, next.name || String(next.id));
  setText(hint, costText(next));
  setText(gold, formatGold(build?.gold));
  lastSignature = signature;

  const boots = build?.bootsRecommendation;
  const showBoots = boots && boots.owned !== true && boots.id !== next.id;
  if (showBoots) {
    const price = boots.upgrade
      ? "mejora gratis"
      : boots.affordable
        ? "listas"
        : `faltan ${formatGold(boots.missingGold)}`;
    setChip(bootsChip, "Botas", `${boots.name} · ${price}`);
  } else {
    setChip(bootsChip, "", "");
  }

  const situational = (build?.situationalItems || []).find(
    (item) => item?.id && item.id !== next.id && item.id !== boots?.id
  );
  if (situational) {
    const price = situational.affordable
      ? "listo"
      : `faltan ${formatGold(situational.missingGold)}`;
    setChip(
      situationalChip,
      situationalLabel(situational.contextNote),
      `${situational.name} · ${price}`,
      true
    );
  } else {
    setChip(situationalChip, "", "");
  }
  showExtras(Boolean(showBoots || situational));
}

function payload(state) {
  return {
    champion: state.champion,
    mode: state.mode || "",
    role: state.role || "MID",
    side: state.side || "blue",
    allies: Array.isArray(state.allies) ? state.allies : [],
    enemies: Array.isArray(state.enemies) ? state.enemies : [],
    owned: Array.isArray(state.owned) ? state.owned : [],
    gold: number(state.gold),
    minute: number(state.minute),
    stable: true,
    game: {
      kills: number(state.kills),
      deaths: number(state.deaths),
      assists: number(state.assists),
      cs: number(state.cs),
      level: number(state.level) || 1,
      gameTime: number(state.gameTime),
      teamKills: number(state.teamKills),
      enemyKills: number(state.enemyKills),
      teamCs: number(state.teamCs),
      enemyCs: number(state.enemyCs),
      teamLevel: number(state.teamLevel),
      enemyLevel: number(state.enemyLevel),
      enemyFed: Array.isArray(state.enemyFed) ? state.enemyFed : [],
      enemyBuilds: Array.isArray(state.enemyBuilds) ? state.enemyBuilds : [],
      gameMode: state.gameMode || "",
      mapName: state.mapName || "",
      queueId: state.queueId ?? null,
    },
    targetItem: null,
    bans: [],
    limit: 5,
    simulate: false,
  };
}

async function request(url, options = {}, timeout = 6000) {
  const controller = new AbortController();
  const timer = setTimeout(() => controller.abort(), timeout);
  try {
    const response = await fetch(url, {
      cache: "no-store",
      ...options,
      signal: controller.signal,
    });
    if (!response.ok) throw new Error(`HTTP ${response.status}`);
    return await response.json();
  } finally {
    clearTimeout(timer);
  }
}

async function ensurePatch() {
  if (patch) return;
  try {
    const health = await request("/api/health", {}, 4000);
    patch = String(health?.patch || "");
  } catch {
    /* se reintenta en el siguiente ciclo */
  }
}

async function tick() {
  if (busy) return;
  busy = true;
  try {
    await ensurePatch();
    const live = await request("/api/client/live", {}, 5000);
    if (!live?.available || !live.state?.champion) {
      waiting();
      return;
    }
    const build = await request(
      "/api/build",
      {
        method: "POST",
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify(payload(live.state)),
      },
      9000
    );
    renderBuild(build);
  } catch {
    if (!hasBuild || !lastSignature) {
      waiting("Sin conexión con el juego", "RiftSense sigue intentando");
    }
  } finally {
    busy = false;
  }
}

async function loop() {
  await tick();
  setTimeout(loop, REFRESH_MS);
}

loop();
