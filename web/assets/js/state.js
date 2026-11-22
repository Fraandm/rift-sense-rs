/* Estado compartido y catalogos hidratados desde /api/meta. */

export const ROLES = ["TOP", "JUNGLE", "MID", "BOT", "SUPPORT"];
export const ROLE_SHORT = { TOP: "TOP", JUNGLE: "JGL", MID: "MID", BOT: "ADC", SUPPORT: "SUP" };

export const state = {
  meta: null,
  activeTab: "draft",
  draft: {
    side: "blue",
    role: "MID",
    allies: [null, null, null, null, null],
    enemies: [null, null, null, null, null],
    bans: [],
  },
  live: {
    champion: null,
    mode: "classic",
    modeLabel: "Grieta del Invocador",
    role: "MID",
    side: "blue",
    allies: [null, null, null, null, null],
    enemies: [null, null, null, null, null],
    owned: [],
    roleQuestBoots: null,
    gold: 0,
    minute: 6,
    game: {
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
      gameMode: "",
      mapName: "",
      queueId: null,
    },
    arenaEnemies: [],
    arenaTeamKnown: false,
  },
};

export const meta = {
  champions: [],
  championsByPresence: [],
  items: [],
  itemsByGold: [],
  byName: new Map(),
  byNorm: new Map(),
  byKey: new Map(),
  itemsById: new Map(),
  itemsByName: new Map(),
  itemsByNorm: new Map(),
  roles: ROLES,
  roleLabels: {},
  iconBase: "",
  patch: "",
};

export function normalize(value) {
  return String(value ?? "")
    .normalize("NFKD")
    .replace(/[\u0300-\u036f]/g, "")
    .replace(/['\u2019`]/g, "")
    .toLowerCase()
    .replace(/[^a-z0-9]+/g, "");
}

export function hydrateMeta(data) {
  state.meta = data;
  meta.patch = data.patch || "";
  meta.iconBase = (data.iconBase || "").replace(/\/$/, "");
  meta.roles = data.roles?.length ? data.roles : ROLES;
  meta.roleLabels = data.roleLabels || {};
  meta.champions = data.champions || [];
  meta.items = data.items || [];

  meta.byName.clear();
  meta.byNorm.clear();
  meta.byKey.clear();
  meta.itemsById.clear();
  meta.itemsByName.clear();
  meta.itemsByNorm.clear();

  for (const champ of meta.champions) {
    meta.byName.set(champ.name, champ);
    meta.byNorm.set(normalize(champ.name), champ);
    meta.byKey.set(Number(champ.key), champ);
  }
  for (const item of meta.items) {
    meta.itemsById.set(Number(item.id), item);
    meta.itemsByName.set(item.name, item);
    meta.itemsByNorm.set(normalize(item.name), item);
  }
  meta.championsByPresence = [...meta.champions].sort(
    (a, b) => (b.presence || 0) - (a.presence || 0) || a.name.localeCompare(b.name)
  );
  meta.itemsByGold = [...meta.items].sort((a, b) => (b.gold || 0) - (a.gold || 0));
}

export function championByName(name) {
  if (!name) return null;
  return meta.byName.get(name) || meta.byNorm.get(normalize(name)) || null;
}

export function itemById(id) {
  return meta.itemsById.get(Number(id)) || null;
}

export function itemByName(name) {
  return meta.itemsByName.get(name) || meta.itemsByNorm.get(normalize(name)) || null;
}

export function roleLabel(role) {
  return meta.roleLabels[role] || role;
}

export function roleIndex(role) {
  return Math.max(0, meta.roles.indexOf(role));
}

export function iconUrl(kind, file) {
  if (!file) return "";
  const base = meta.iconBase || (meta.patch ? `https://ddragon.leagueoflegends.com/cdn/${meta.patch}/img` : "");
  return base ? `${base}/${kind}/${file}` : "";
}

export function championRoleGames(champ, role) {
  return champ?.roles?.[role]?.n || 0;
}

export function championRoleWinrate(champ, role) {
  return champ?.roles?.[role]?.winrate ?? null;
}
