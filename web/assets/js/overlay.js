/* Overlay compacto: objetivo legendario y alternativas de la vista En vivo. */

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
const optionOne = $("option-one");
const optionTwo = $("option-two");

let patch = "";
let busy = false;

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
  setChip(optionOne, "", "");
  setChip(optionTwo, "", "");
  showExtras(false);
}

function costText(item, build) {
  if (!item) return "";
  if (item.needsSlot && number(build?.slot) >= (number(build?.slotCap) || 6)) return "Falta un hueco";
  if (item.affordable) return "Listo para comprar";
  return `Faltan ${formatGold(item.missingGold)}`;
}

function renderComplete(build) {
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
  setChip(optionOne, "", "");
  setChip(optionTwo, "", "");
  showExtras(false);
}

function renderBuild(build) {
  const next = build?.nextLegendary || null;
  if (!next && (build?.mode === "complete" || build?.fullInventory)) {
    renderComplete(build);
    return;
  }
  if (!next) {
    waiting("Sin objetivo legendario", "El motor no encontró una opción ahora mismo");
    return;
  }

  overlay.classList.remove("waiting", "complete");
  art.classList.remove("empty", "done");
  pulse.classList.add("hidden");

  const champion = build?.champion?.name || build?.champion || "";
  const role = ROLE_LABEL[build?.role] || build?.role || "";
  setImage(iconUrl("item", next.icon), next.name);
  setText(eyebrow, "OBJETIVO LEGENDARIO");
  setText(champ, [champion, role].filter(Boolean).join(" · "));
  setText(itemName, next.name || String(next.id));
  setText(hint, costText(next, build));
  setText(gold, formatGold(build?.gold));

  const options = Array.isArray(build?.legendaryOptions) ? build.legendaryOptions.slice(0, 2) : [];
  [optionOne, optionTwo].forEach((element, index) => {
    const option = options[index];
    setChip(element, option ? `Opción ${index + 1}` : "", option ? `${option.name} · ${costText(option, build)}` : "");
  });
  showExtras(options.length > 0);
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
    const build = await request("/api/overlay/snapshot", {}, 5000);
    if (!build) {
      waiting("Esperando sincronización", "Abre En vivo en RiftSense");
      return;
    }
    renderBuild(build);
  } catch {
    // Nunca conservar una compra antigua cuando no podemos verificarla.
    waiting("Sin sincronización", "RiftSense sigue intentando");
  } finally {
    busy = false;
  }
}

async function loop() {
  await tick();
  setTimeout(loop, REFRESH_MS);
}

loop();
