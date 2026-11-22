/* Historial local de partidas: compras, recomendaciones y resultado. */

import { itemById, meta } from "./state.js";

const KEY = "riftsense.sessions.v1";
const CLEARED_KEY = "riftsense.sessions.cleared";
const MAX_SESSIONS = 20;
let current = null;
let lastOwned = new Set();
let lastUpdated = 0;

function read() {
  try {
    const value = JSON.parse(localStorage.getItem(KEY) || "[]");
    return Array.isArray(value) ? value : [];
  } catch {
    return [];
  }
}

function write(sessions) {
  try {
    localStorage.setItem(KEY, JSON.stringify(sessions.slice(0, MAX_SESSIONS)));
  } catch {
    /* sin almacenamiento */
  }
}

function persist(session) {
  const sessions = read().filter((entry) => entry.id !== session.id);
  sessions.unshift(session);
  write(sessions);
}

function newSession({ champion, mode, role, side, gameId }) {
  return {
    id: `${Date.now()}-${Math.random().toString(36).slice(2, 8)}`,
    gameId: gameId ?? null,
    startedAt: new Date().toISOString(),
    updatedAt: new Date().toISOString(),
    champion: champion || "",
    mode: mode || "classic",
    role: role || "MID",
    side: side || "blue",
    result: null,
    resultSource: null,
    durationSeconds: 0,
    kda: { kills: 0, deaths: 0, assists: 0, cs: 0, level: 1 },
    finalItems: [],
    purchases: [],
    lastRecommendation: null,
    augments: [],
    augmentIds: [],
    anvils: [],
    allies: [],
    followed: 0,
    planned: 0,
    off: 0,
    finalized: false,
    source: "live",
  };
}

function aggregate(session) {
  session.purchases = Array.isArray(session.purchases) ? session.purchases : [];
  const counted = session.purchases.filter((purchase) => !purchase.initial);
  session.followed = counted.filter((purchase) => purchase.followed).length;
  session.planned = counted.filter((purchase) => !purchase.followed && purchase.planned).length;
  session.off = counted.filter((purchase) => !purchase.followed && !purchase.planned).length;
  return session;
}

export function activeSession() {
  return current && !current.finalized ? current : null;
}

export function startSession(info) {
  current = newSession(info);
  lastOwned = new Set();
  lastUpdated = Date.now();
  persist(current);
  return current;
}

export function recordSession({
  champion,
  mode,
  role,
  side,
  minute,
  owned,
  build,
  augments,
  anvils,
  allies,
  kda,
  gameId,
}) {
  if (!champion) return null;
  const now = Date.now();
  if (!current && gameId != null) {
    const stored = read()
      .filter(
        (entry) =>
          entry.source === "live" &&
          !entry.finalized &&
          String(entry.gameId || "") === String(gameId)
      )
      .sort((a, b) => String(b.updatedAt).localeCompare(String(a.updatedAt)))[0];
    if (stored) {
      current = stored;
      lastOwned = new Set((stored.finalItems || []).map(Number));
      lastUpdated = Date.parse(stored.updatedAt) || now;
    }
  }
  const expired = current && now - lastUpdated > 30 * 60 * 1000;
  const gameChanged =
    current?.gameId != null &&
    gameId != null &&
    String(current.gameId) !== String(gameId);
  if (
    !current ||
    current.finalized ||
    expired ||
    gameChanged ||
    current.champion !== champion ||
    current.mode !== mode
  ) {
    startSession({ champion, mode, role, side, gameId });
  }

  current.purchases = Array.isArray(current.purchases) ? current.purchases : [];
  current.finalItems = Array.isArray(current.finalItems) ? current.finalItems : [];
  const currentOwned = new Set((owned || []).map(Number));
  const initial = lastOwned.size === 0 && current.purchases.length === 0;
  for (const id of currentOwned) {
    if (lastOwned.has(id)) continue;
    const item = itemById(id);
    const previous = current.lastRecommendation;
    const followed = Boolean(previous && previous.id === Number(id));
    const planned = Boolean(
      previous && Array.isArray(previous.plan) && previous.plan.includes(Number(id))
    );
    current.purchases.push({
      id: Number(id),
      name: item?.name || `Objeto ${id}`,
      icon: item?.icon || `${id}.png`,
      minute: Number(minute) || 0,
      followed,
      planned,
      initial,
    });
  }
  lastOwned = currentOwned;
  current.finalItems = [...currentOwned];
  current.updatedAt = new Date().toISOString();
  current.durationSeconds = Math.max(current.durationSeconds, (Number(minute) || 0) * 60);
  current.kda = {
    kills: Number(kda?.kills) || 0,
    deaths: Number(kda?.deaths) || 0,
    assists: Number(kda?.assists) || 0,
    cs: Number(kda?.cs) || 0,
    level: Number(kda?.level) || 1,
  };
  current.augments = Array.isArray(augments) ? [...augments] : [];
  current.anvils = Array.isArray(anvils) ? [...anvils] : [];
  current.allies = Array.isArray(allies) ? allies.filter(Boolean) : [];
  current.lastRecommendation = build?.nextPurchase
    ? {
        id: Number(build.nextPurchase.id),
        name: build.nextPurchase.name,
        plan: [
          ...(build.items || []).map((item) => Number(item.id)),
          ...(build.buildPlan || []).map((item) => Number(item.id)),
        ],
      }
    : null;
  current.role = role || current.role;
  current.side = side || current.side;
  current.gameId = gameId ?? current.gameId ?? null;
  aggregate(current);
  lastUpdated = now;
  persist(current);
  return current;
}

function applyPostgame(session, postgame) {
  if (postgame.gameId != null) session.gameId = postgame.gameId;
  if (postgame.finishedAt) session.finishedAt = postgame.finishedAt;
  if (postgame.result === "win" || postgame.result === "loss") {
    session.result = postgame.result;
    session.resultSource = "lcu";
  }
  if (Number(postgame.durationSeconds) > 0) {
    session.durationSeconds = Number(postgame.durationSeconds);
  }
  if (Array.isArray(postgame.items) && postgame.items.length) {
    const items = postgame.items.map(Number);
    const known = items.filter((id) => itemById(id));
    session.finalItems = known.length ? known : items;
  }
  if (Array.isArray(postgame.augmentIds)) {
    session.augmentIds = postgame.augmentIds.filter((id) => Number(id) > 0).map(Number);
  }
  if (postgame.kills != null) {
    session.kda = {
      kills: Number(postgame.kills) || 0,
      deaths: Number(postgame.deaths) || 0,
      assists: Number(postgame.assists) || 0,
      cs: Number(postgame.cs) || 0,
      level: Number(postgame.level) || 0,
    };
  }
}

export function finalizeSession(postgame = null) {
  if (!current) return null;
  const session = current;
  if (postgame?.available) applyPostgame(session, postgame);
  session.finalized = true;
  session.endedAt = new Date().toISOString();
  session.updatedAt = session.endedAt;
  aggregate(session);
  persist(session);
  current = session;
  return session;
}

export function setManualResult(id, win) {
  const sessions = read();
  const session = sessions.find((entry) => entry.id === id);
  if (!session) return null;
  session.result = win ? "win" : "loss";
  session.resultSource = "manual";
  session.updatedAt = new Date().toISOString();
  write(sessions);
  if (current?.id === id) {
    current.result = session.result;
    current.resultSource = "manual";
  }
  return session;
}

function modeFromGameMode(value) {
  const key = String(value || "").toUpperCase();
  if (key === "KIWI") return "mayhem";
  if (key === "CHERRY") return "arena";
  if (key === "ARAM") return "aram";
  return "classic";
}

/// Importa la ultima partida del historial del cliente cuando la app no pudo
/// seguirla en vivo. Mantiene el resultado y la build real, aunque no haya
/// secuencia de compras.
export function importPostgame(postgame) {
  if (!postgame?.available) return null;
  try {
    const marker = JSON.parse(localStorage.getItem(CLEARED_KEY) || "null");
    if (marker && Number(marker.at) > 0) {
      const finished = Date.parse(postgame.finishedAt || "");
      if (Number.isFinite(finished) && finished <= Number(marker.at)) return null;
      if (
        !Number.isFinite(finished) &&
        marker.gameId != null &&
        postgame.gameId != null &&
        Number(postgame.gameId) <= Number(marker.gameId)
      ) {
        return null;
      }
    }
  } catch {
    /* marcador ausente o invalido */
  }
  const sessions = read();
  if (postgame.gameId != null) {
    const existing = sessions.find(
      (entry) => String(entry.gameId || "") === String(postgame.gameId)
    );
    if (existing) {
      applyPostgame(existing, postgame);
      existing.finalized = true;
      existing.endedAt = existing.endedAt || new Date().toISOString();
      existing.updatedAt = new Date().toISOString();
      aggregate(existing);
      persist(existing);
      return existing;
    }
  }
  const championId = Number(postgame.championId);
  const champion =
    (Number.isFinite(championId) && meta.byKey.get(championId)?.name) ||
    (Number.isFinite(championId) ? `Campeón ${championId}` : "Partida reciente");
  const finishedAt = Date.parse(postgame.finishedAt || "");
  const tracked = sessions.find((entry) => {
    if (entry.source !== "live" || entry.gameId != null || entry.champion !== champion) {
      return false;
    }
    const stamp = Date.parse(entry.endedAt || entry.updatedAt || "");
    return (
      !Number.isFinite(finishedAt) ||
      !Number.isFinite(stamp) ||
      Math.abs(finishedAt - stamp) < 15 * 60 * 1000
    );
  });
  if (tracked) {
    applyPostgame(tracked, postgame);
    tracked.finalized = true;
    tracked.endedAt = tracked.endedAt || new Date().toISOString();
    tracked.updatedAt = new Date().toISOString();
    aggregate(tracked);
    persist(tracked);
    return tracked;
  }
  const session = newSession({
    champion,
    mode: modeFromGameMode(postgame.gameMode),
    role: "",
    side: "",
  });
  if (Number.isFinite(finishedAt)) {
    const duration = Number(postgame.durationSeconds) || 0;
    session.startedAt = new Date(finishedAt - duration * 1000).toISOString();
    session.endedAt = new Date(finishedAt).toISOString();
  }
  applyPostgame(session, postgame);
  session.resultSource = session.result ? "lcu" : null;
  session.source = "match-history";
  session.finalized = true;
  session.endedAt = session.endedAt || new Date().toISOString();
  session.updatedAt = new Date().toISOString();
  persist(session);
  return session;
}

export function loadSessions() {
  return read().sort((a, b) => String(b.startedAt).localeCompare(String(a.startedAt)));
}

export function clearSessions() {
  const gameId = read().reduce((latest, session) => {
    const value = Number(session.gameId);
    return Number.isFinite(value) && value > latest ? value : latest;
  }, 0);
  write([]);
  try {
    localStorage.setItem(
      CLEARED_KEY,
      JSON.stringify({ at: Date.now(), gameId: gameId || null })
    );
  } catch {
    /* sin almacenamiento */
  }
  current = null;
  lastOwned = new Set();
  lastUpdated = 0;
}
