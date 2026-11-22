use anyhow::{Result, bail};
use indexmap::IndexMap;
use serde::Deserialize;
use serde_json::{Map, Value, json};
use std::collections::{HashMap, HashSet};

use crate::config::Paths;
use crate::data::{Catalog, Champion, Item, MetaItem, norm_name};
use crate::model::{BuildModel, DraftModel};

#[derive(Deserialize, Clone, Debug)]
#[serde(untagged)]
pub enum ChampionRef {
    Int(i64),
    Str(String),
}

#[derive(Deserialize, Clone, Debug)]
#[serde(untagged)]
pub enum ItemRef {
    Int(i64),
    Str(String),
}

#[derive(Deserialize, Clone, Debug)]
#[serde(untagged)]
pub enum SideRef {
    Int(i64),
    Str(String),
}

impl Default for SideRef {
    fn default() -> Self {
        Self::Str("blue".to_string())
    }
}

fn default_role() -> String {
    "MID".to_string()
}

fn default_draft_limit() -> usize {
    10
}

fn default_ban_limit() -> usize {
    8
}

fn default_true() -> bool {
    true
}

#[derive(Deserialize, Clone, Debug)]
pub struct DraftRequest {
    #[serde(default = "default_role")]
    pub role: String,
    #[serde(default)]
    pub side: SideRef,
    #[serde(default)]
    pub bans: Vec<ChampionRef>,
    #[serde(default)]
    pub allies: Vec<Option<ChampionRef>>,
    #[serde(default)]
    pub enemies: Vec<Option<ChampionRef>>,
    #[serde(default = "default_draft_limit")]
    pub limit: usize,
}

#[derive(Deserialize, Clone, Debug)]
pub struct BansRequest {
    #[serde(default)]
    pub exclude: Vec<ChampionRef>,
    #[serde(default = "default_ban_limit")]
    pub limit: usize,
}

#[derive(Deserialize, Clone, Debug, Default)]
#[serde(rename_all = "camelCase")]
pub struct GameContext {
    #[serde(default)]
    pub kills: i64,
    #[serde(default)]
    pub deaths: i64,
    #[serde(default)]
    pub assists: i64,
    #[serde(default)]
    pub cs: i64,
    #[serde(default)]
    pub level: i64,
    #[serde(default)]
    pub game_time: f64,
    #[serde(default)]
    pub team_kills: i64,
    #[serde(default)]
    pub enemy_kills: i64,
    #[serde(default)]
    pub team_cs: i64,
    #[serde(default)]
    pub enemy_cs: i64,
    #[serde(default)]
    pub team_level: f64,
    #[serde(default)]
    pub enemy_level: f64,
    #[serde(default)]
    pub enemy_fed: Vec<String>,
}

#[derive(Deserialize, Clone, Debug)]
pub struct BuildRequest {
    pub champion: ChampionRef,
    #[serde(default = "default_role")]
    pub role: String,
    #[serde(default)]
    pub side: SideRef,
    #[serde(default)]
    pub allies: Vec<Option<ChampionRef>>,
    #[serde(default)]
    pub enemies: Vec<Option<ChampionRef>>,
    #[serde(default)]
    pub owned: Vec<ItemRef>,
    #[serde(default)]
    pub gold: i64,
    #[serde(default)]
    pub minute: f64,
    #[serde(default)]
    pub game: Option<GameContext>,
    #[serde(default = "default_draft_limit")]
    pub limit: usize,
    #[serde(default = "default_true")]
    pub simulate: bool,
}

#[derive(Deserialize, Clone, Debug)]
pub struct LiveRequest {
    #[serde(flatten)]
    pub build: BuildRequest,
    #[serde(default)]
    pub bans: Vec<ChampionRef>,
}

#[derive(Clone, Debug)]
struct ChampEdge {
    n: i64,
    winrate: f64,
    edge: f64,
    role_pick_rate: f64,
    total_games: i64,
    presence: f64,
}

#[derive(Clone, Debug)]
struct MatchupNote {
    player: String,
    winrate: f64,
    n: i64,
}

#[derive(Clone, Debug)]
struct MatchupEdge {
    winrate: f64,
    n: i64,
    edge: f64,
    notes: Vec<MatchupNote>,
}

#[derive(Clone, Debug)]
struct ItemPriorInfo {
    n: i64,
    wins: i64,
    avg_slot: f64,
    scope: &'static str,
}

#[derive(Clone, Debug)]
struct ItemCandidate<'a> {
    item: &'a Item,
    model_prob: f64,
    prior_n: i64,
    avg_slot: f64,
    slot_fit: f64,
    winrate: f64,
    wr_edge: f64,
    defense: f64,
    defense_note: String,
    context_fit: f64,
    context_note: String,
    score: f64,
    affordable: bool,
    missing_gold: i64,
}

#[derive(Clone, Debug)]
struct GameAnalysis {
    state: &'static str,
    phase: &'static str,
    kills: i64,
    deaths: i64,
    assists: i64,
    cs: i64,
    level: i64,
    game_time: f64,
    team_kills: i64,
    enemy_kills: i64,
    team_cs: i64,
    enemy_cs: i64,
    team_level: f64,
    enemy_level: f64,
    kill_diff: i64,
    cs_diff: i64,
    level_diff: f64,
    kda: f64,
    cs_per_min: f64,
    kill_participation: f64,
    enemy_fed: Vec<String>,
    healers: Vec<String>,
    notes: Vec<String>,
}

impl GameAnalysis {
    fn to_json(&self) -> Value {
        json!({
            "state": self.state,
            "label": match self.state {
                "ahead" => "Vas por delante",
                "behind" => "Vas por detrás",
                _ => "Partida igualada",
            },
            "phase": self.phase,
            "phaseLabel": match self.phase {
                "early" => "Early game",
                "late" => "Late game",
                _ => "Mid game",
            },
            "kills": self.kills,
            "deaths": self.deaths,
            "assists": self.assists,
            "kda": round(self.kda, 2),
            "cs": self.cs,
            "csPerMin": round(self.cs_per_min, 1),
            "level": self.level,
            "gameTime": round(self.game_time, 1),
            "teamKills": self.team_kills,
            "enemyKills": self.enemy_kills,
            "killDiff": self.kill_diff,
            "teamCs": self.team_cs,
            "enemyCs": self.enemy_cs,
            "csDiff": self.cs_diff,
            "teamLevel": round(self.team_level, 1),
            "enemyLevel": round(self.enemy_level, 1),
            "levelDiff": round(self.level_diff, 1),
            "killParticipation": round(self.kill_participation, 3),
            "enemyFed": self.enemy_fed,
            "healingThreat": self.healers,
            "notes": self.notes,
        })
    }
}

const HEALING_CHAMPIONS: &[&str] = &[
    "aatrox",
    "briar",
    "darius",
    "drmundo",
    "fiora",
    "gwen",
    "illaoi",
    "irelia",
    "jax",
    "kayn",
    "kled",
    "maokai",
    "mordekaiser",
    "nami",
    "nasus",
    "nidalee",
    "olaf",
    "rakan",
    "rengar",
    "riven",
    "samira",
    "seraphine",
    "sett",
    "shen",
    "sona",
    "soraka",
    "sylas",
    "tahmkench",
    "taric",
    "trundle",
    "tryndamere",
    "udyr",
    "vladimir",
    "volibear",
    "warwick",
    "yuumi",
    "zac",
];

const GRIEVOUS_ITEMS: &[&str] = &[
    "morellonomicon",
    "chempunkchainsword",
    "chemtechputrifier",
    "mortalreminder",
    "thornmail",
    "oblivionorb",
    "executionerscalling",
    "bramblevest",
];

fn compact_name(value: &str) -> String {
    value
        .to_lowercase()
        .chars()
        .filter(|character| character.is_ascii_alphanumeric())
        .collect()
}

fn is_healing_champion(name: &str) -> bool {
    let key = compact_name(name);
    HEALING_CHAMPIONS.iter().any(|candidate| *candidate == key)
}

fn is_grievous_item(item: &Item) -> bool {
    let key = compact_name(&item.name);
    GRIEVOUS_ITEMS
        .iter()
        .any(|candidate| key.contains(candidate))
}

fn is_defensive_item(item: &Item) -> bool {
    let tags: HashSet<&str> = item.tags.iter().map(String::as_str).collect();
    let stat = |key: &str| item.stats.get(key).copied().unwrap_or(0.0);
    tags.contains("Armor")
        || tags.contains("SpellBlock")
        || tags.contains("Health")
        || stat("FlatArmorMod") > 0.0
        || stat("FlatSpellBlockMod") > 0.0
        || stat("FlatHPPoolMod") > 0.0
}

fn is_offensive_item(item: &Item) -> bool {
    let tags: HashSet<&str> = item.tags.iter().map(String::as_str).collect();
    let stat = |key: &str| item.stats.get(key).copied().unwrap_or(0.0);
    tags.contains("Damage")
        || tags.contains("AbilityPower")
        || tags.contains("AttackSpeed")
        || tags.contains("CriticalStrike")
        || stat("FlatPhysicalDamageMod") > 0.0
        || stat("FlatMagicDamageMod") > 0.0
}

#[derive(Clone, Debug, Default)]
struct DraftTensors {
    champ: Vec<i64>,
    role: Vec<i64>,
    side: Vec<i64>,
    kind: Vec<i64>,
    mask: Vec<bool>,
}

pub struct Engine {
    pub paths: Paths,
    pub vocab: crate::data::Vocab,
    pub priors: crate::data::Priors,
    pub meta: Value,
    pub summary: Value,
    pub draft_report: Value,
    pub build_report: Value,
    pub timings: Value,
    pub champions: Vec<Champion>,
    pub items: Vec<Item>,
    pub roles: Vec<String>,
    pub role_labels: HashMap<String, String>,
    pub role_idx: HashMap<String, usize>,
    pub unk: i64,
    pub champ_by_norm: HashMap<String, usize>,
    pub champ_by_key: HashMap<i64, usize>,
    pub item_by_id: HashMap<i64, usize>,
    pub item_by_norm: HashMap<String, usize>,
    pub all_items_by_id: HashMap<i64, MetaItem>,
    pub priors_games: f64,
    pub patch: String,
    pub icon_base: String,
    pub draft_model: Option<DraftModel>,
    pub build_model: Option<BuildModel>,
    pub model_notes: Vec<String>,
}

impl Engine {
    pub fn load(paths: Paths) -> Result<Self> {
        let catalog = Catalog::load(&paths)?;
        let champion_count = catalog.champions.len() as i64;
        let item_count = catalog.items.len() as i64;
        let manifest = crate::data::read_manifest(&paths.models);

        let mut model_notes = Vec::new();
        let mut draft_model = None;
        let mut build_model = None;

        if paths.draft_model().is_file() {
            match DraftModel::load(&paths.draft_model()) {
                Ok(model) => {
                    let expected = manifest
                        .as_ref()
                        .and_then(|m| m.get("draft_n_champions"))
                        .and_then(Value::as_i64);
                    if expected.is_some() && expected != Some(champion_count) {
                        model_notes.push(format!(
                            "DraftNet incompatible: vocabulario {} vs modelo {}.",
                            champion_count,
                            expected.unwrap_or_default()
                        ));
                    } else {
                        draft_model = Some(model);
                    }
                }
                Err(error) => model_notes.push(format!("No se pudo cargar DraftNet ONNX: {error}")),
            }
        } else {
            model_notes.push("DraftNet no entrenada: se usan estadísticas empíricas.".to_string());
        }

        if paths.build_model().is_file() {
            match BuildModel::load(&paths.build_model()) {
                Ok(model) => {
                    let expected = manifest
                        .as_ref()
                        .and_then(|m| m.get("build_n_items"))
                        .and_then(Value::as_i64);
                    if expected.is_some() && expected != Some(item_count) {
                        model_notes.push(format!(
                            "BuildNet incompatible: vocabulario {} vs modelo {}.",
                            item_count,
                            expected.unwrap_or_default()
                        ));
                    } else {
                        build_model = Some(model);
                    }
                }
                Err(error) => model_notes.push(format!("No se pudo cargar BuildNet ONNX: {error}")),
            }
        } else {
            model_notes.push("BuildNet no entrenada: se usan estadísticas empíricas.".to_string());
        }

        Ok(Self {
            paths,
            vocab: catalog.vocab,
            priors: catalog.priors,
            meta: catalog.meta,
            summary: catalog.summary,
            draft_report: catalog.draft_report,
            build_report: catalog.build_report,
            timings: catalog.timings,
            champions: catalog.champions,
            items: catalog.items,
            roles: catalog.roles,
            role_labels: catalog.role_labels,
            role_idx: catalog.role_idx,
            unk: catalog.unk,
            champ_by_norm: catalog.champ_by_norm,
            champ_by_key: catalog.champ_by_key,
            item_by_id: catalog.item_by_id,
            item_by_norm: catalog.item_by_norm,
            all_items_by_id: catalog.all_items_by_id,
            priors_games: catalog.priors_games,
            patch: catalog.patch,
            icon_base: catalog.icon_base,
            draft_model,
            build_model,
            model_notes,
        })
    }

    pub fn resolve_champion(&self, value: &ChampionRef) -> Option<&Champion> {
        match value {
            ChampionRef::Int(key) => self
                .champ_by_key
                .get(key)
                .or_else(|| self.champ_by_norm.get(&norm_name(&key.to_string())))
                .map(|index| &self.champions[*index]),
            ChampionRef::Str(text) => {
                let text = text.trim();
                if text.is_empty() {
                    return None;
                }
                self.champ_by_norm
                    .get(&norm_name(text))
                    .or_else(|| {
                        text.parse::<i64>()
                            .ok()
                            .and_then(|key| self.champ_by_key.get(&key))
                    })
                    .map(|index| &self.champions[*index])
            }
        }
    }

    pub fn resolve_item(&self, value: &ItemRef) -> Option<&Item> {
        match value {
            ItemRef::Int(id) => self.item_by_id.get(id).map(|index| &self.items[*index]),
            ItemRef::Str(text) => {
                let text = text.trim();
                if text.is_empty() {
                    return None;
                }
                if let Ok(id) = text.parse::<i64>() {
                    if let Some(index) = self.item_by_id.get(&id) {
                        return Some(&self.items[*index]);
                    }
                }
                self.item_by_norm
                    .get(&norm_name(text))
                    .map(|index| &self.items[*index])
            }
        }
    }

    pub fn resolve_side(&self, value: &SideRef) -> Result<i32> {
        match value {
            SideRef::Int(value) => Ok(if *value == 0 { 0 } else { 1 }),
            SideRef::Str(text) => match text.trim().to_lowercase().as_str() {
                "blue" | "azul" | "0" => Ok(0),
                "red" | "rojo" | "1" => Ok(1),
                _ => bail!("Lado inválido: usa 'blue'/'red' o 0/1"),
            },
        }
    }

    pub fn resolve_role(&self, value: &str) -> Result<String> {
        let text = value.trim().to_lowercase();
        let mapped = match text.as_str() {
            "top" => Some("TOP"),
            "jgl" | "jungle" | "jungla" => Some("JUNGLE"),
            "mid" | "middle" => Some("MID"),
            "bot" | "adc" | "tirador" => Some("BOT"),
            "support" | "sup" | "soporte" => Some("SUPPORT"),
            _ => None,
        };
        let role = mapped
            .map(str::to_string)
            .or_else(|| {
                let upper = text.to_uppercase();
                if self.roles.contains(&upper) {
                    Some(upper)
                } else {
                    None
                }
            })
            .filter(|role| self.roles.contains(role));
        match role {
            Some(role) => Ok(role),
            None => bail!("Rol inválido: {value}"),
        }
    }

    fn team<'a>(&'a self, values: &[Option<ChampionRef>]) -> Result<Vec<Option<&'a Champion>>> {
        let mut team = Vec::with_capacity(5);
        let mut seen = HashSet::new();
        for index in 0..5 {
            let champ = values
                .get(index)
                .and_then(|value| value.as_ref())
                .and_then(|value| self.resolve_champion(value));
            if let Some(champ) = champ {
                if !seen.insert(champ.key) {
                    bail!("Campeón duplicado: {}", champ.name);
                }
            }
            team.push(champ);
        }
        Ok(team)
    }

    fn ban_list<'a>(&'a self, bans: &[ChampionRef]) -> Vec<&'a Champion> {
        let mut result = Vec::new();
        let mut seen = HashSet::new();
        for value in bans.iter().take(10) {
            if let Some(champ) = self.resolve_champion(value) {
                if seen.insert(champ.key) {
                    result.push(champ);
                }
            }
        }
        result
    }

    fn champ_edge(&self, name: &str, role: &str) -> ChampEdge {
        let data = self.priors.champions.get(name);
        let role_data = data.and_then(|data| data.roles.get(role));
        let baseline = self
            .priors
            .role_baseline
            .get(role)
            .map(|baseline| baseline.winrate)
            .unwrap_or(0.5);
        let role_games = self
            .priors
            .role_baseline
            .get(role)
            .map(|baseline| baseline.n)
            .unwrap_or(0)
            .max(1);
        let n = role_data.map(|data| data.n).unwrap_or(0);
        let wins = role_data.map(|data| data.wins).unwrap_or(0);
        let wr = if n > 0 {
            (wins as f64 + baseline * 80.0) / (n as f64 + 80.0)
        } else {
            baseline
        };
        let total_games = data.map(|data| data.n).unwrap_or(0);
        let bans = data.map(|data| data.bans).unwrap_or(0);
        ChampEdge {
            n,
            winrate: wr,
            edge: wr - baseline,
            role_pick_rate: n as f64 / role_games as f64,
            total_games,
            presence: (total_games + bans) as f64 / self.priors_games,
        }
    }

    fn matchup_edge(&self, name: &str, enemies: &[Option<&Champion>]) -> MatchupEdge {
        let data = self.priors.champions.get(name);
        let empty = IndexMap::new();
        let matchups = data.map(|data| &data.matchups).unwrap_or(&empty);
        let mut n_total = 0i64;
        let mut w_total = 0.0;
        let mut notes = Vec::new();
        for enemy in enemies {
            let Some(enemy) = enemy else { continue };
            let Some(entry) = matchups.get(&enemy.name) else {
                continue;
            };
            if entry.n <= 0 {
                continue;
            }
            let wr = (entry.wins as f64 + 0.5 * 20.0) / (entry.n as f64 + 20.0);
            n_total += entry.n;
            w_total += wr * entry.n as f64;
            notes.push(MatchupNote {
                player: enemy.name.clone(),
                winrate: wr,
                n: entry.n,
            });
        }
        if n_total == 0 {
            return MatchupEdge {
                winrate: 0.5,
                n: 0,
                edge: 0.0,
                notes: Vec::new(),
            };
        }
        let wr = w_total / n_total as f64;
        let confidence = n_total as f64 / (n_total as f64 + 80.0);
        notes.sort_by(|a, b| b.n.cmp(&a.n));
        notes.truncate(4);
        MatchupEdge {
            winrate: wr,
            n: n_total,
            edge: (wr - 0.5) * confidence,
            notes,
        }
    }

    fn synergy_edge(&self, name: &str, allies: &[Option<&Champion>]) -> MatchupEdge {
        let data = self.priors.champions.get(name);
        let empty = IndexMap::new();
        let synergy = data.map(|data| &data.synergy).unwrap_or(&empty);
        let mut n_total = 0i64;
        let mut w_total = 0.0;
        let mut notes = Vec::new();
        for ally in allies {
            let Some(ally) = ally else { continue };
            if ally.name == name {
                continue;
            }
            let Some(entry) = synergy.get(&ally.name) else {
                continue;
            };
            if entry.n <= 0 {
                continue;
            }
            let wr = (entry.wins as f64 + 0.5 * 20.0) / (entry.n as f64 + 20.0);
            n_total += entry.n;
            w_total += wr * entry.n as f64;
            notes.push(MatchupNote {
                player: ally.name.clone(),
                winrate: wr,
                n: entry.n,
            });
        }
        if n_total == 0 {
            return MatchupEdge {
                winrate: 0.5,
                n: 0,
                edge: 0.0,
                notes: Vec::new(),
            };
        }
        let wr = w_total / n_total as f64;
        let confidence = n_total as f64 / (n_total as f64 + 80.0);
        notes.sort_by(|a, b| b.n.cmp(&a.n));
        notes.truncate(4);
        MatchupEdge {
            winrate: wr,
            n: n_total,
            edge: (wr - 0.5) * confidence,
            notes,
        }
    }

    fn draft_inputs(
        &self,
        side: i32,
        allies: &[Option<&Champion>],
        enemies: &[Option<&Champion>],
        bans: &[&Champion],
    ) -> DraftTensors {
        let mut champ = vec![0i64; 20];
        let mut role = vec![0i64; 20];
        let mut side_ids = vec![0i64; 20];
        let mut kind = vec![0i64; 20];
        let mut mask = vec![false; 20];

        let (team1, team2) = if side == 0 {
            (allies, enemies)
        } else {
            (enemies, allies)
        };
        for (slot, team) in [(0usize, team1), (1usize, team2)] {
            for r in 0..5 {
                let pos = slot * 5 + r;
                champ[pos] = team
                    .get(r)
                    .and_then(|c| *c)
                    .map(|c| c.idx)
                    .unwrap_or(self.unk);
                role[pos] = r as i64 + 1;
                side_ids[pos] = slot as i64;
                kind[pos] = 1;
                mask[pos] = true;
            }
        }
        for (i, ban) in bans.iter().take(10).enumerate() {
            let pos = 10 + i;
            champ[pos] = ban.idx;
            side_ids[pos] = if i < 5 { 0 } else { 1 };
            kind[pos] = 2;
            mask[pos] = true;
        }
        DraftTensors {
            champ,
            role,
            side: side_ids,
            kind,
            mask,
        }
    }

    fn predict_draft(&self, tensors: &DraftTensors, n: usize) -> Result<Vec<f32>> {
        if let Some(model) = &self.draft_model {
            model.predict(
                n,
                &tensors.champ,
                &tensors.role,
                &tensors.side,
                &tensors.kind,
                &tensors.mask,
            )
        } else {
            Ok(vec![0.5; n])
        }
    }

    pub fn draft_recommend(&self, req: &DraftRequest) -> Result<Value> {
        let role = self.resolve_role(&req.role)?;
        let side = self.resolve_side(&req.side)?;
        let ally_team = self.team(&req.allies)?;
        let enemy_team = self.team(&req.enemies)?;
        let ban_list = self.ban_list(&req.bans);
        let limit = req.limit.clamp(1, 25);

        let picked: HashSet<i64> = ally_team
            .iter()
            .chain(enemy_team.iter())
            .filter_map(|champ| champ.map(|champ| champ.key))
            .collect();
        let used: HashSet<i64> = picked
            .iter()
            .copied()
            .chain(ban_list.iter().map(|champ| champ.key))
            .collect();
        let my_role_idx = *self
            .role_idx
            .get(&role)
            .ok_or_else(|| anyhow::anyhow!("Rol sin indice: {role}"))?;
        let my_slot = (if side == 0 { 0 } else { 5 }) + my_role_idx - 1;

        let tensors = self.draft_inputs(side, &ally_team, &enemy_team, &ban_list);
        let base_p = self.predict_draft(&tensors, 1)?[0] as f64;
        let base_user_p = if side == 0 { base_p } else { 1.0 - base_p };
        let mode = if self.draft_model.is_some() {
            "model"
        } else {
            "priors"
        };

        let my_champ = ally_team.get(my_role_idx - 1).and_then(|champ| *champ);
        if let Some(my_champ) = my_champ {
            let edge = self.champ_edge(&my_champ.name, &role);
            let exclude = self.exclude_names(&ban_list, &ally_team, &enemy_team);
            let mut notes = self.model_notes.clone();
            notes.push(
                "Tu campeón ya está fijado: quítalo del slot para comparar alternativas."
                    .to_string(),
            );
            return Ok(json!({
                "mode": mode,
                "role": role,
                "roleLabel": self.role_labels.get(&role).cloned().unwrap_or_else(|| role.clone()),
                "side": if side == 0 { "blue" } else { "red" },
                "baselineWinProbability": round(base_user_p, 4),
                "myChampion": champion_brief(my_champ),
                "myChampionStats": {
                    "winrate": round(edge.winrate, 4),
                    "games": edge.n,
                    "presence": round(edge.presence, 4),
                },
                "knownAllies": team_names(&ally_team),
                "knownEnemies": team_names(&enemy_team),
                "bans": ban_list.iter().map(|champ| champ.name.clone()).collect::<Vec<_>>(),
                "recommendations": [],
                "banPriority": self.ban_priority(&exclude, 8),
                "notes": notes,
            }));
        }

        let mut candidates: Vec<(&Champion, ChampEdge)> = Vec::new();
        for champ in &self.champions {
            if used.contains(&champ.key) {
                continue;
            }
            let edge = self.champ_edge(&champ.name, &role);
            if edge.total_games > 0 && edge.n == 0 {
                continue;
            }
            if edge.total_games < 3 {
                continue;
            }
            let role_share = edge.n as f64 / edge.total_games.max(1) as f64;
            if edge.n < 100 || role_share < 0.10 {
                continue;
            }
            candidates.push((champ, edge));
        }
        if candidates.len() < 6 {
            candidates.clear();
            for champ in &self.champions {
                if used.contains(&champ.key) {
                    continue;
                }
                let edge = self.champ_edge(&champ.name, &role);
                let role_share = edge.n as f64 / edge.total_games.max(1) as f64;
                if edge.n >= 20 && role_share >= 0.05 {
                    candidates.push((champ, edge));
                }
            }
        }
        if candidates.is_empty() {
            bail!("No hay campeones candidatos con datos suficientes.");
        }

        let batch_n = candidates.len();
        let mut champ = Vec::with_capacity(batch_n * 20);
        let mut role_ids = Vec::with_capacity(batch_n * 20);
        let mut sides = Vec::with_capacity(batch_n * 20);
        let mut kinds = Vec::with_capacity(batch_n * 20);
        let mut mask = Vec::with_capacity(batch_n * 20);
        for _ in 0..batch_n {
            champ.extend_from_slice(&tensors.champ);
            role_ids.extend_from_slice(&tensors.role);
            sides.extend_from_slice(&tensors.side);
            kinds.extend_from_slice(&tensors.kind);
            mask.extend_from_slice(&tensors.mask);
        }
        for (i, (champ_info, _)) in candidates.iter().enumerate() {
            champ[i * 20 + my_slot] = champ_info.idx;
        }
        let batch_tensors = DraftTensors {
            champ,
            role: role_ids,
            side: sides,
            kind: kinds,
            mask,
        };
        let probs = self.predict_draft(&batch_tensors, batch_n)?;
        let user_probs: Vec<f64> = probs
            .into_iter()
            .map(|p| {
                let p = p as f64;
                if side == 0 { p } else { 1.0 - p }
            })
            .collect();
        let model_edges: Vec<f64> = user_probs.iter().map(|p| p - base_user_p).collect();
        let prior_edges: Vec<f64> = candidates.iter().map(|(_, edge)| edge.edge).collect();
        let matchups: Vec<MatchupEdge> = candidates
            .iter()
            .map(|(champ, _)| self.matchup_edge(&champ.name, &enemy_team))
            .collect();
        let synergies: Vec<MatchupEdge> = candidates
            .iter()
            .map(|(champ, _)| self.synergy_edge(&champ.name, &ally_team))
            .collect();
        let presence: Vec<f64> = candidates.iter().map(|(_, edge)| edge.presence).collect();
        let role_pick: Vec<f64> = candidates
            .iter()
            .map(|(_, edge)| edge.role_pick_rate)
            .collect();

        let has_model = self.draft_model.is_some();
        let mut weights = if has_model {
            [
                ("model", 0.35),
                ("prior", 0.25),
                (
                    "matchup",
                    if enemy_team.iter().any(Option::is_some) {
                        0.15
                    } else {
                        0.0
                    },
                ),
                (
                    "synergy",
                    if ally_team.iter().any(Option::is_some) {
                        0.10
                    } else {
                        0.0
                    },
                ),
                ("meta", 0.15),
            ]
        } else {
            [
                ("model", 0.0),
                ("prior", 0.35),
                (
                    "matchup",
                    if enemy_team.iter().any(Option::is_some) {
                        0.15
                    } else {
                        0.0
                    },
                ),
                (
                    "synergy",
                    if ally_team.iter().any(Option::is_some) {
                        0.10
                    } else {
                        0.0
                    },
                ),
                ("meta", 0.40),
            ]
        };
        let total_w: f64 = weights.iter().map(|(_, v)| *v).sum();
        for (_, value) in &mut weights {
            *value /= if total_w == 0.0 { 1.0 } else { total_w };
        }
        let weight = |name: &str| {
            weights
                .iter()
                .find(|(key, _)| *key == name)
                .map(|(_, value)| *value)
                .unwrap_or(0.0)
        };

        let z_model = standardize(&model_edges);
        let z_prior = standardize(&prior_edges);
        let z_matchup = standardize(
            &matchups
                .iter()
                .map(|matchup| matchup.edge)
                .collect::<Vec<_>>(),
        );
        let z_synergy = standardize(
            &synergies
                .iter()
                .map(|synergy| synergy.edge)
                .collect::<Vec<_>>(),
        );
        let z_meta_base = standardize(&presence);
        let z_meta_pick = standardize(&role_pick);
        let z_meta: Vec<f64> = z_meta_base
            .iter()
            .zip(z_meta_pick.iter())
            .map(|(a, b)| 0.5 * a + 0.5 * b)
            .collect();

        let known_picks = ally_team
            .iter()
            .chain(enemy_team.iter())
            .filter(|c| c.is_some())
            .count();
        let model_confidence = 0.25 + 0.75 * (known_picks as f64 / 10.0);

        let mut rows: Vec<(f64, Value)> = Vec::with_capacity(candidates.len());
        for (i, (champ, edge)) in candidates.iter().enumerate() {
            let composite = weight("model") * z_model[i] * model_confidence
                + weight("prior") * z_prior[i]
                + weight("matchup") * z_matchup[i]
                + weight("synergy") * z_synergy[i]
                + weight("meta") * z_meta[i];
            let score = clamp(50.0 + 18.0 * composite, 1.0, 99.0);
            let matchup = &matchups[i];
            let synergy = &synergies[i];
            let mut reasons = Vec::new();
            if edge.n >= 10 {
                reasons.push(format!(
                    "Winrate {} en {} ({} partidas pro)",
                    pct(edge.winrate, 1),
                    self.role_labels
                        .get(&role)
                        .cloned()
                        .unwrap_or_else(|| role.clone()),
                    edge.n
                ));
            }
            if matchup.n >= 15 && matchup.winrate >= 0.515 {
                if let Some(best) = matchup.notes.first() {
                    reasons.push(format!(
                        "Buen matchup contra {} ({}, n={})",
                        best.player,
                        pct(best.winrate, 0),
                        best.n
                    ));
                }
            }
            if synergy.n >= 15 && synergy.winrate >= 0.515 {
                if let Some(best) = synergy.notes.first() {
                    reasons.push(format!(
                        "Sinergia con {} ({}, n={})",
                        best.player,
                        pct(best.winrate, 0),
                        best.n
                    ));
                }
            }
            if edge.presence >= 0.08 {
                reasons.push(format!("Presencia en draft: {}", pct(edge.presence, 0)));
            }
            if has_model {
                let delta = model_edges[i] * 100.0;
                if delta.abs() >= 0.05 {
                    reasons.push(format!(
                        "Modelo: {} de victoria ({:+.1} pp)",
                        pct(user_probs[i], 1),
                        delta
                    ));
                }
            }
            reasons.truncate(4);
            let value = json!({
                "champion": champion_brief(champ),
                "role": role,
                "score": round(score, 1),
                "winProbability": round(user_probs[i], 4),
                "deltaProbability": round(model_edges[i], 4),
                "evidence": {
                    "modelEdge": round(model_edges[i], 4),
                    "priorWinrate": round(edge.winrate, 4),
                    "priorGames": edge.n,
                    "presence": round(edge.presence, 4),
                    "rolePickRate": round(edge.role_pick_rate, 4),
                    "matchupWinrate": round(matchup.winrate, 4),
                    "matchupGames": matchup.n,
                    "synergyWinrate": round(synergy.winrate, 4),
                    "synergyGames": synergy.n,
                },
                "reasons": reasons,
            });
            rows.push((round(score, 1), value));
        }
        rows.sort_by(|a, b| b.0.partial_cmp(&a.0).unwrap_or(std::cmp::Ordering::Equal));
        let recommendations: Vec<Value> = rows
            .into_iter()
            .take(limit)
            .map(|(_, value)| value)
            .collect();
        let exclude = self.exclude_names(&ban_list, &ally_team, &enemy_team);

        Ok(json!({
            "mode": mode,
            "role": role,
            "roleLabel": self.role_labels.get(&role).cloned().unwrap_or_else(|| role.clone()),
            "side": if side == 0 { "blue" } else { "red" },
            "baselineWinProbability": round(base_user_p, 4),
            "knownAllies": team_names(&ally_team),
            "knownEnemies": team_names(&enemy_team),
            "bans": ban_list.iter().map(|champ| champ.name.clone()).collect::<Vec<_>>(),
            "recommendations": recommendations,
            "banPriority": self.ban_priority(&exclude, 8),
            "notes": self.model_notes,
        }))
    }

    pub fn draft_evaluate(&self, req: &DraftRequest) -> Result<Value> {
        let role = self.resolve_role(&req.role)?;
        let side = self.resolve_side(&req.side)?;
        let ally_team = self.team(&req.allies)?;
        let enemy_team = self.team(&req.enemies)?;
        let ban_list = self.ban_list(&req.bans);
        let tensors = self.draft_inputs(side, &ally_team, &enemy_team, &ban_list);
        let p = self.predict_draft(&tensors, 1)?[0] as f64;
        let p_user = if side == 0 { p } else { 1.0 - p };
        let ally_edge = self.team_edge(&ally_team);
        let enemy_edge = self.team_edge(&enemy_team);
        let p_prior = clamp(0.5 + 1.2 * (ally_edge - enemy_edge), 0.03, 0.97);
        let has_model = self.draft_model.is_some();
        let p_final = if has_model {
            let shrunk = 0.5 + 0.65 * (p_user - 0.5);
            clamp(0.6 * shrunk + 0.4 * p_prior, 0.03, 0.97)
        } else {
            p_prior
        };
        let my_champ = ally_team
            .get(self.role_idx[&role] - 1)
            .and_then(|champ| *champ);
        let known = ally_team
            .iter()
            .chain(enemy_team.iter())
            .filter(|c| c.is_some())
            .count();
        Ok(json!({
            "mode": if has_model { "model" } else { "priors" },
            "winProbability": round(p_final, 4),
            "modelProbability": if has_model { json!(round(p_user, 4)) } else { Value::Null },
            "priorProbability": round(p_prior, 4),
            "myChampion": my_champ.map(champion_brief).unwrap_or(Value::Null),
            "allyEdge": round(ally_edge, 4),
            "enemyEdge": round(enemy_edge, 4),
            "knownPicks": known,
            "complete": known == 10,
            "notes": self.model_notes,
        }))
    }

    fn team_edge(&self, team: &[Option<&Champion>]) -> f64 {
        let mut values = Vec::new();
        for (index, champ) in team.iter().enumerate() {
            if let Some(champ) = champ {
                if let Some(role) = self.roles.get(index) {
                    values.push(self.champ_edge(&champ.name, role).edge);
                }
            }
        }
        if values.is_empty() {
            0.0
        } else {
            values.iter().sum::<f64>() / values.len() as f64
        }
    }

    pub fn ban_priority(&self, exclude: &[String], limit: usize) -> Vec<Value> {
        let excluded: HashSet<String> = exclude.iter().map(|name| norm_name(name)).collect();
        let mut rows: Vec<(f64, Value)> = Vec::new();
        for (name, data) in &self.priors.champions {
            if excluded.contains(&norm_name(name)) {
                continue;
            }
            let presence = (data.n + data.bans) as f64 / self.priors_games;
            rows.push((
                presence,
                json!({
                    "name": name,
                    "presence": presence,
                    "winrate": data.winrate,
                    "games": data.n,
                    "bans": data.bans,
                }),
            ));
        }
        if rows.is_empty() {
            return Vec::new();
        }
        let presence_values: Vec<f64> = rows.iter().map(|(presence, _)| *presence).collect();
        let winrate_values: Vec<f64> = rows
            .iter()
            .map(|(_, row)| row.get("winrate").and_then(Value::as_f64).unwrap_or(0.5))
            .collect();
        let pres_z = standardize(&presence_values);
        let wr_z = standardize(&winrate_values);
        let mut output = Vec::with_capacity(rows.len());
        for (i, (_, mut row)) in rows.drain(..).enumerate() {
            let priority = clamp(50.0 + 18.0 * (0.65 * pres_z[i] + 0.35 * wr_z[i]), 1.0, 99.0);
            let priority = round(priority, 1);
            let name = row
                .get("name")
                .and_then(Value::as_str)
                .unwrap_or("")
                .to_string();
            if let Some(champ) = self.resolve_champion(&ChampionRef::Str(name.clone())) {
                row["icon"] = json!(champ.icon);
                row["key"] = json!(champ.key);
            }
            row["priority"] = json!(priority);
            let presence = row.get("presence").and_then(Value::as_f64).unwrap_or(0.0);
            let winrate = row.get("winrate").and_then(Value::as_f64).unwrap_or(0.5);
            row["reason"] = json!(format!(
                "Presencia {} · winrate {}",
                pct(presence, 0),
                pct(winrate, 1)
            ));
            output.push((priority, row));
        }
        output.sort_by(|a, b| b.0.partial_cmp(&a.0).unwrap_or(std::cmp::Ordering::Equal));
        output
            .into_iter()
            .take(limit.clamp(1, 20))
            .map(|(_, row)| row)
            .collect()
    }

    #[allow(clippy::too_many_arguments)]
    fn score_items<'a>(
        &'a self,
        champ: &Champion,
        role: &str,
        side: i32,
        ally_team: &[Option<&Champion>],
        enemy_team: &[Option<&Champion>],
        owned: &[&'a Item],
        gold: i64,
        _minute: f64,
        analysis: Option<&GameAnalysis>,
        use_model: bool,
    ) -> Result<Vec<ItemCandidate<'a>>> {
        let owned_ids: HashSet<i64> = owned.iter().map(|item| item.id).collect();
        let slot = owned.len();
        let profile = self.enemy_profile(enemy_team);

        let mut items_tensor = [0i64; 7];
        for (i, item) in owned.iter().take(7).enumerate() {
            items_tensor[i] = item.idx;
        }
        let ally_ids = [
            ally_team
                .get(0)
                .and_then(|c| *c)
                .map(|c| c.idx)
                .unwrap_or(0),
            ally_team
                .get(1)
                .and_then(|c| *c)
                .map(|c| c.idx)
                .unwrap_or(0),
            ally_team
                .get(2)
                .and_then(|c| *c)
                .map(|c| c.idx)
                .unwrap_or(0),
            ally_team
                .get(3)
                .and_then(|c| *c)
                .map(|c| c.idx)
                .unwrap_or(0),
            ally_team
                .get(4)
                .and_then(|c| *c)
                .map(|c| c.idx)
                .unwrap_or(0),
        ];
        let enemy_ids = [
            enemy_team
                .get(0)
                .and_then(|c| *c)
                .map(|c| c.idx)
                .unwrap_or(0),
            enemy_team
                .get(1)
                .and_then(|c| *c)
                .map(|c| c.idx)
                .unwrap_or(0),
            enemy_team
                .get(2)
                .and_then(|c| *c)
                .map(|c| c.idx)
                .unwrap_or(0),
            enemy_team
                .get(3)
                .and_then(|c| *c)
                .map(|c| c.idx)
                .unwrap_or(0),
            enemy_team
                .get(4)
                .and_then(|c| *c)
                .map(|c| c.idx)
                .unwrap_or(0),
        ];

        let probs = if use_model {
            if let Some(model) = &self.build_model {
                model.predict(
                    champ.idx,
                    *self.role_idx.get(role).unwrap_or(&1) as i64,
                    side as i64,
                    &ally_ids,
                    &enemy_ids,
                    &items_tensor,
                    slot as i64,
                )?
            } else {
                vec![0.0; self.items.len() + 1]
            }
        } else {
            vec![0.0; self.items.len() + 1]
        };

        let mut candidates = Vec::new();
        for item in &self.items {
            let item_id = item.id;
            if owned_ids.contains(&item_id) {
                continue;
            }
            if slot >= 6 {
                continue;
            }
            if item.boots && owned.iter().any(|owned| owned.boots) {
                continue;
            }
            let prior = self.item_prior(&champ.name, role, item_id);
            let avg_slot = prior.avg_slot;
            let slot_fit = if prior.scope == "none" {
                0.30
            } else {
                (-0.5 * ((avg_slot - slot as f64) / 1.6).powi(2)).exp()
            };
            let wr = (prior.wins as f64 + 0.5 * 25.0) / (prior.n as f64 + 25.0);
            let wr_edge = wr - 0.5;
            let (defense, defense_note) = self.defensive_fit(item, &profile);
            let (context_fit, context_note) = analysis
                .map(|analysis| self.context_fit(item, analysis, &profile, owned))
                .unwrap_or((0.0, String::new()));
            candidates.push(ItemCandidate {
                item,
                model_prob: probs.get(item.idx as usize).copied().unwrap_or(0.0) as f64,
                prior_n: prior.n,
                avg_slot,
                slot_fit,
                winrate: wr,
                wr_edge,
                defense,
                defense_note,
                context_fit,
                context_note,
                score: 0.0,
                affordable: false,
                missing_gold: 0,
            });
        }
        if candidates.is_empty() {
            return Ok(candidates);
        }

        let model_weight = if use_model && self.build_model.is_some() {
            0.50
        } else {
            0.0
        };
        let mut weights = vec![
            ("model", model_weight),
            ("popularity", 0.20),
            ("slot", 0.15),
            ("winrate", 0.05),
            ("defense", 0.10),
        ];
        if analysis.is_some() {
            weights.push(("context", 0.12));
        }
        let total_w: f64 = weights.iter().map(|(_, value)| *value).sum();
        for (_, value) in &mut weights {
            *value /= if total_w == 0.0 { 1.0 } else { total_w };
        }
        let weight = |name: &str| {
            weights
                .iter()
                .find(|(key, _)| *key == name)
                .map(|(_, value)| *value)
                .unwrap_or(0.0)
        };

        let max_prior = candidates
            .iter()
            .map(|candidate| candidate.prior_n)
            .max()
            .unwrap_or(1)
            .max(1) as f64;
        let z_model = standardize(
            &candidates
                .iter()
                .map(|candidate| candidate.model_prob)
                .collect::<Vec<_>>(),
        );
        let z_pop = standardize(
            &candidates
                .iter()
                .map(|candidate| candidate.prior_n as f64 / max_prior)
                .collect::<Vec<_>>(),
        );
        let z_slot = standardize(
            &candidates
                .iter()
                .map(|candidate| candidate.slot_fit)
                .collect::<Vec<_>>(),
        );
        let z_wr = standardize(
            &candidates
                .iter()
                .map(|candidate| candidate.wr_edge)
                .collect::<Vec<_>>(),
        );
        let z_def = standardize(
            &candidates
                .iter()
                .map(|candidate| candidate.defense)
                .collect::<Vec<_>>(),
        );
        let z_context = standardize(
            &candidates
                .iter()
                .map(|candidate| candidate.context_fit)
                .collect::<Vec<_>>(),
        );
        for (i, candidate) in candidates.iter_mut().enumerate() {
            let composite = weight("model") * z_model[i]
                + weight("popularity") * z_pop[i]
                + weight("slot") * z_slot[i]
                + weight("winrate") * z_wr[i]
                + weight("defense") * z_def[i]
                + weight("context") * z_context[i];
            candidate.score = clamp(50.0 + 11.0 * composite, 1.0, 96.0);
            candidate.affordable = gold >= candidate.item.gold;
            candidate.missing_gold = (candidate.item.gold - gold).max(0);
        }
        candidates.sort_by(|a, b| {
            b.score
                .partial_cmp(&a.score)
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        Ok(candidates)
    }

    fn enemy_profile(&self, enemies: &[Option<&Champion>]) -> Value {
        let mut ad = 0.0;
        let mut ap = 0.0;
        for champ in enemies.iter().flatten() {
            let info = &champ.info;
            let tags: HashSet<&str> = champ.tags.iter().map(String::as_str).collect();
            let mut a = info.attack as f64;
            let mut m = info.magic as f64;
            if tags.contains("Marksman") || tags.contains("Fighter") || tags.contains("Assassin") {
                a += 2.0;
            }
            if tags.contains("Mage") {
                m += 2.0;
            }
            match champ.damage.as_str() {
                "AD" => a += 2.0,
                "AP" => m += 2.0,
                _ => {}
            }
            ad += a;
            ap += m;
        }
        let total = ad + ap;
        if total <= 0.0 {
            return json!({
                "physical": 0.5,
                "magic": 0.5,
                "label": "Mixta",
                "known": 0,
            });
        }
        let physical = ad / total;
        let label = if physical >= 0.62 {
            "AD"
        } else if physical <= 0.38 {
            "AP"
        } else {
            "Mixta"
        };
        json!({
            "physical": round(physical, 3),
            "magic": round(1.0 - physical, 3),
            "label": label,
            "known": enemies.iter().filter(|champ| champ.is_some()).count(),
        })
    }

    fn item_prior(&self, champ_name: &str, role: &str, item_id: i64) -> ItemPriorInfo {
        let champ_data = self.priors.champions.get(champ_name);
        let champ_games = champ_data
            .and_then(|data| data.roles.get(role))
            .map(|data| data.n)
            .unwrap_or(0);
        let entry = champ_data
            .and_then(|data| data.items.get(role))
            .and_then(|items| items.get(&item_id.to_string()));
        if let Some(entry) = entry {
            return ItemPriorInfo {
                n: entry.n,
                wins: entry.wins,
                avg_slot: entry.avg_slot,
                scope: "champ",
            };
        }
        if champ_games >= 50 {
            return ItemPriorInfo {
                n: 0,
                wins: 0,
                avg_slot: 0.0,
                scope: "none",
            };
        }
        let role_items = self
            .priors
            .items
            .get(&item_id.to_string())
            .and_then(|item| item.roles.get(role));
        if let Some(role_items) = role_items {
            return ItemPriorInfo {
                n: role_items.n,
                wins: (role_items.n as f64 * role_items.winrate) as i64,
                avg_slot: role_items.avg_slot,
                scope: "role",
            };
        }
        ItemPriorInfo {
            n: 0,
            wins: 0,
            avg_slot: 0.0,
            scope: "none",
        }
    }

    fn analyze_game(
        &self,
        context: &GameContext,
        minute: f64,
        enemies: &[Option<&Champion>],
    ) -> Option<GameAnalysis> {
        let has_signals = context.game_time > 0.0
            || context.cs > 0
            || context.level > 1
            || context.team_kills > 0
            || context.enemy_kills > 0
            || context.team_cs > 0
            || context.enemy_cs > 0;
        if !has_signals {
            return None;
        }

        let minute = if context.game_time > 0.0 {
            context.game_time / 60.0
        } else {
            minute
        };
        let kda = (context.kills + context.assists) as f64 / context.deaths.max(1) as f64;
        let cs_per_min = context.cs as f64 / minute.max(1.0);
        let kill_diff = context.team_kills - context.enemy_kills;
        let cs_diff = context.team_cs - context.enemy_cs;
        let level_diff = context.team_level - context.enemy_level;
        let kill_participation = if context.team_kills > 0 {
            ((context.kills + context.assists) as f64 / context.team_kills as f64).clamp(0.0, 1.0)
        } else {
            0.0
        };
        let tempo = kill_diff as f64 * 2.0
            + level_diff * 3.0
            + cs_diff as f64 / 80.0
            + (kda - 2.0).clamp(-2.0, 2.0) * 0.3;
        let state = if tempo >= 3.0 {
            "ahead"
        } else if tempo <= -3.0 {
            "behind"
        } else {
            "even"
        };
        let phase = if minute < 14.0 {
            "early"
        } else if minute < 28.0 {
            "mid"
        } else {
            "late"
        };

        let mut healers = Vec::new();
        for champion in enemies.iter().flatten() {
            if is_healing_champion(&champion.name) {
                healers.push(champion.name.clone());
            }
        }
        healers.sort();
        healers.dedup();

        let enemy_fed: Vec<String> = context
            .enemy_fed
            .iter()
            .filter(|name| {
                enemies
                    .iter()
                    .flatten()
                    .any(|champion| champion.name.eq_ignore_ascii_case(name))
            })
            .take(3)
            .cloned()
            .collect();

        let mut notes = vec![
            format!(
                "Equipos {}-{} ({:+})",
                context.team_kills, context.enemy_kills, kill_diff
            ),
            format!(
                "CS {}-{} ({:+})",
                context.team_cs, context.enemy_cs, cs_diff
            ),
            format!(
                "Nivel medio {:.1}-{:.1} ({:+.1})",
                context.team_level, context.enemy_level, level_diff
            ),
            format!("KDA {:.1} · {:.1} CS/min", kda, cs_per_min),
        ];
        if !healers.is_empty() {
            notes.push(format!("Curación enemiga: {}", healers.join(", ")));
        }
        if !enemy_fed.is_empty() {
            notes.push(format!("Rivales fuertes: {}", enemy_fed.join(", ")));
        }

        Some(GameAnalysis {
            state,
            phase,
            kills: context.kills,
            deaths: context.deaths,
            assists: context.assists,
            cs: context.cs,
            level: context.level,
            game_time: context.game_time,
            team_kills: context.team_kills,
            enemy_kills: context.enemy_kills,
            team_cs: context.team_cs,
            enemy_cs: context.enemy_cs,
            team_level: context.team_level,
            enemy_level: context.enemy_level,
            kill_diff,
            cs_diff,
            level_diff,
            kda,
            cs_per_min,
            kill_participation,
            enemy_fed,
            healers,
            notes,
        })
    }

    fn context_fit(
        &self,
        item: &Item,
        analysis: &GameAnalysis,
        profile: &Value,
        owned: &[&Item],
    ) -> (f64, String) {
        let tags: HashSet<&str> = item.tags.iter().map(String::as_str).collect();
        let stat = |key: &str| item.stats.get(key).copied().unwrap_or(0.0);
        let defensive = is_defensive_item(item);
        let offensive = is_offensive_item(item);
        let armor = tags.contains("Armor") || stat("FlatArmorMod") > 0.0;
        let magic_resist = tags.contains("SpellBlock") || stat("FlatSpellBlockMod") > 0.0;
        let physical = profile
            .get("physical")
            .and_then(Value::as_f64)
            .unwrap_or(0.5);
        let magic = profile.get("magic").and_then(Value::as_f64).unwrap_or(0.5);
        let mut score = 0.0;
        let mut notes = Vec::new();

        if analysis.state == "behind" {
            if defensive {
                score += 0.20;
                notes.push("Vas por detrás: prioriza resistencias");
            }
            if offensive {
                score -= 0.05;
            }
        } else if analysis.state == "ahead" {
            if offensive {
                score += 0.10;
                notes.push("Ventaja: acelera tu spike ofensivo");
            }
            if defensive {
                score -= 0.03;
            }
        }

        if !analysis.healers.is_empty()
            && is_grievous_item(item)
            && !owned.iter().any(|owned| is_grievous_item(owned))
        {
            score += 0.35;
            notes.push("Antiheal contra la curación rival");
        }

        if !analysis.enemy_fed.is_empty() {
            if physical >= 0.55 && armor {
                score += 0.10;
                notes.push("Armadura contra el rival fuerte");
            }
            if magic >= 0.55 && magic_resist {
                score += 0.10;
                notes.push("Resistencia mágica contra el rival fuerte");
            }
        }

        if analysis.phase == "late" && item.gold >= 2800 {
            score += 0.04;
        }

        (clamp(score, -0.1, 0.55), notes.join(" · "))
    }

    fn defensive_fit(&self, item: &Item, profile: &Value) -> (f64, String) {
        let tags: HashSet<&str> = item.tags.iter().map(String::as_str).collect();
        let stat = |key: &str| item.stats.get(key).copied().unwrap_or(0.0);
        let armor = stat("FlatArmorMod") + if tags.contains("Armor") { 1.0 } else { 0.0 };
        let mr = stat("FlatSpellBlockMod")
            + if tags.contains("SpellBlock") {
                1.0
            } else {
                0.0
            };
        let hp = stat("FlatHPPoolMod");
        let physical = profile
            .get("physical")
            .and_then(Value::as_f64)
            .unwrap_or(0.5);
        let magic = profile.get("magic").and_then(Value::as_f64).unwrap_or(0.5);
        let label = profile
            .get("label")
            .and_then(Value::as_str)
            .unwrap_or("Mixta");
        let mut score = 0.0;
        let mut note = String::new();
        if armor != 0.0 && physical >= 0.55 {
            score = (armor / 60.0).min(1.3) * (physical - 0.5) * 2.0;
            note = "armadura contra daño físico".to_string();
        }
        if mr != 0.0 && magic >= 0.55 {
            let mr_score = (mr / 55.0).min(1.3) * (magic - 0.5) * 2.0;
            if mr_score > score {
                score = mr_score;
                note = "resistencia mágica contra daño AP".to_string();
            }
        }
        if hp != 0.0 && label == "Mixta" && hp >= 300.0 {
            let hp_score = (hp / 700.0).min(1.0) * 0.5;
            if hp_score > score {
                score = hp_score;
                note = "vida contra daño mixto".to_string();
            }
        }
        (clamp(score, 0.0, 1.5), note)
    }

    fn components(&self, item: &Item, gold: i64) -> Vec<Value> {
        let mut components: Vec<Value> = Vec::new();
        for id in &item.from {
            let (name, icon, cost) = if let Some(meta) = self.all_items_by_id.get(id) {
                (meta.name.clone(), meta.icon.clone(), meta.gold)
            } else if let Some(index) = self.item_by_id.get(id) {
                let comp = &self.items[*index];
                (comp.name.clone(), comp.icon.clone(), comp.gold)
            } else {
                continue;
            };
            components.push(json!({
                "id": id,
                "name": name,
                "icon": icon,
                "gold": cost,
                "affordable": gold >= cost,
            }));
        }
        components.sort_by(|a, b| {
            b.get("gold")
                .and_then(Value::as_i64)
                .unwrap_or(0)
                .cmp(&a.get("gold").and_then(Value::as_i64).unwrap_or(0))
        });
        components.truncate(5);
        components
    }

    fn format_item(&self, candidate: &ItemCandidate, gold: i64, slot: usize, role: &str) -> Value {
        let item = candidate.item;
        let prior_n = candidate.prior_n;
        let avg_slot = candidate.avg_slot;
        let typical_minute = self
            .timings
            .get(item.id.to_string())
            .and_then(|value| value.get(role))
            .and_then(|value| value.get(slot.to_string()))
            .and_then(|value| value.get("median"))
            .and_then(Value::as_f64)
            .filter(|value| *value != 0.0);
        let mut reasons = Vec::new();
        if prior_n >= 10 {
            reasons.push(format!("Aparece en {prior_n} builds analizadas"));
            if avg_slot != 0.0 {
                reasons.push(format!(
                    "Posición media en inventario: {:.1}",
                    avg_slot + 1.0
                ));
            }
        }
        if let Some(minute) = typical_minute {
            reasons.push(format!("Minuto típico: ~{minute:.1}"));
        }
        if candidate.winrate >= 0.515 && prior_n >= 20 {
            reasons.push(format!(
                "Winrate {} con este objeto",
                pct(candidate.winrate, 1)
            ));
        }
        if !candidate.defense_note.is_empty() {
            reasons.push(format!("Aporta {}", candidate.defense_note));
        }
        if !candidate.context_note.is_empty() {
            reasons.push(candidate.context_note.clone());
        }
        if candidate.affordable {
            reasons.push("Puedes comprarlo ahora".to_string());
        } else {
            reasons.push(format!("Te faltan {} de oro", candidate.missing_gold));
        }
        let components = self.components(item, gold);
        if !candidate.affordable {
            if let Some(cheap) = components.iter().find(|component| {
                component.get("affordable").and_then(Value::as_bool) == Some(true)
            }) {
                if let Some(name) = cheap.get("name").and_then(Value::as_str) {
                    reasons.push(format!("Componente asequible: {name}"));
                }
            }
        }
        json!({
            "id": item.id,
            "name": item.name,
            "icon": item.icon,
            "gold": item.gold,
            "tags": item.tags,
            "score": round(candidate.score, 1),
            "affordable": candidate.affordable,
            "missingGold": candidate.missing_gold,
            "avgSlot": if prior_n > 0 { json!(round(avg_slot + 1.0, 2)) } else { Value::Null },
            "typicalMinute": json!(typical_minute.map(|value| round(value, 2))),
            "priorGames": prior_n,
            "winrate": round(candidate.winrate, 4),
            "modelProb": round(candidate.model_prob, 5),
            "components": components,
            "reasons": reasons,
            "slot": slot + 1,
        })
    }

    fn next_component(
        &self,
        candidates: &[&ItemCandidate],
        gold: i64,
        owned_ids: &HashSet<i64>,
    ) -> Value {
        for candidate in candidates.iter().take(5) {
            for component in self.components(candidate.item, gold) {
                let affordable = component
                    .get("affordable")
                    .and_then(Value::as_bool)
                    .unwrap_or(false);
                let id = component.get("id").and_then(Value::as_i64).unwrap_or(0);
                if affordable && id > 0 && !owned_ids.contains(&id) {
                    return json!({
                        "id": id,
                        "name": component.get("name").cloned().unwrap_or(Value::Null),
                        "icon": component.get("icon").cloned().unwrap_or(Value::Null),
                        "gold": component.get("gold").cloned().unwrap_or(json!(0)),
                        "affordable": true,
                        "parent": candidate.item.name,
                    });
                }
            }
        }
        Value::Null
    }

    pub fn build_recommend(&self, req: &BuildRequest) -> Result<Value> {
        let champ = self
            .resolve_champion(&req.champion)
            .ok_or_else(|| anyhow::anyhow!("Campeón desconocido: {:?}", req.champion))?;
        let role = self.resolve_role(&req.role)?;
        let side = self.resolve_side(&req.side)?;
        let ally_team = self.team(&req.allies)?;
        let enemy_team = self.team(&req.enemies)?;
        let gold = req.gold.clamp(0, 100_000);
        let minute = req.minute.clamp(0.0, 120.0);
        let limit = req.limit.clamp(1, 25);

        let mut owned_items: Vec<&Item> = Vec::new();
        let mut seen = HashSet::new();
        for value in req.owned.iter().take(8) {
            if let Some(item) = self.resolve_item(value) {
                if seen.insert(item.id) {
                    owned_items.push(item);
                }
            }
        }
        let slot = owned_items.len().min(6);
        let profile = self.enemy_profile(&enemy_team);
        let analysis = req
            .game
            .as_ref()
            .and_then(|game| self.analyze_game(game, minute, &enemy_team));
        if slot >= 6 {
            let mut complete = json!({
                "mode": "complete",
                "champion": champion_brief(champ),
                "role": role,
                "slot": 6,
                "enemyProfile": profile,
                "items": [],
                "buildPlan": [],
                "notes": ["Build completa: 6 objetos. Considera el trinket y consumibles."],
            });
            if let Some(analysis) = analysis.as_ref() {
                complete["gameAnalysis"] = analysis.to_json();
            }
            return Ok(complete);
        }

        let candidates = self.score_items(
            champ,
            &role,
            side,
            &ally_team,
            &enemy_team,
            &owned_items,
            gold,
            minute,
            analysis.as_ref(),
            true,
        )?;
        let items_out: Vec<Value> = candidates
            .iter()
            .take(limit)
            .map(|candidate| self.format_item(candidate, gold, slot, &role))
            .collect();
        let focus: Vec<&ItemCandidate> = if let Some(analysis) = analysis.as_ref() {
            let mut preferred = if !analysis.healers.is_empty()
                && !owned_items.iter().any(|item| is_grievous_item(item))
            {
                candidates
                    .iter()
                    .filter(|candidate| is_grievous_item(candidate.item))
                    .collect::<Vec<_>>()
            } else if analysis.state == "behind" {
                candidates
                    .iter()
                    .filter(|candidate| is_defensive_item(candidate.item))
                    .collect::<Vec<_>>()
            } else if analysis.state == "ahead" {
                candidates
                    .iter()
                    .filter(|candidate| is_offensive_item(candidate.item))
                    .collect::<Vec<_>>()
            } else {
                candidates.iter().collect::<Vec<_>>()
            };
            if preferred.is_empty() {
                preferred = candidates.iter().collect();
            }
            preferred.sort_by(|left, right| {
                right
                    .context_fit
                    .partial_cmp(&left.context_fit)
                    .unwrap_or(std::cmp::Ordering::Equal)
                    .then_with(|| {
                        right
                            .score
                            .partial_cmp(&left.score)
                            .unwrap_or(std::cmp::Ordering::Equal)
                    })
            });
            preferred
        } else {
            candidates.iter().collect()
        };
        let next_purchase = focus
            .iter()
            .find(|candidate| gold >= candidate.item.gold)
            .map(|candidate| self.format_item(candidate, gold, slot, &role));

        let mut build_plan = Vec::new();
        if req.simulate {
            let mut sim_owned = owned_items.clone();
            let mut sim_gold = gold;
            for _ in 0..(6 - slot) {
                let sims = self.score_items(
                    champ,
                    &role,
                    side,
                    &ally_team,
                    &enemy_team,
                    &sim_owned,
                    sim_gold,
                    minute,
                    analysis.as_ref(),
                    true,
                )?;
                let Some(best) = sims.first() else { break };
                let item = best.item;
                let mut reasons = vec![format!("Score {:.0}", best.score)];
                if best.prior_n >= 20 {
                    reasons.push(format!("Winrate {}", pct(best.winrate, 1)));
                }
                build_plan.push(json!({
                    "id": item.id,
                    "name": item.name,
                    "icon": item.icon,
                    "gold": item.gold,
                    "slot": sim_owned.len() + 1,
                    "score": round(best.score, 1),
                    "reasons": reasons,
                }));
                sim_owned.push(item);
                sim_gold = (sim_gold - item.gold).max(0);
            }
        }

        let item_count = &self.priors.item_count_winrate;
        let spike = item_count
            .get(&(slot + 1).to_string())
            .or_else(|| item_count.get(&slot.to_string()));
        let mut nearby = Map::new();
        for key in slot..(slot + 3).min(8) {
            if let Some(value) = item_count.get(&key.to_string()) {
                nearby.insert(
                    key.to_string(),
                    json!({"n": value.n, "winrate": value.winrate}),
                );
            }
        }
        let mut notes = self.model_notes.clone();
        if candidates.is_empty() {
            notes.push("No hay objetos candidatos con los filtros actuales.".to_string());
        }
        if profile.get("known").and_then(Value::as_i64).unwrap_or(0) < 5 {
            notes.push("Faltan campeones enemigos: el perfil de daño es aproximado.".to_string());
        }

        let next_component = if analysis.is_some() && next_purchase.is_none() {
            self.next_component(&focus, gold, &seen)
        } else {
            Value::Null
        };
        let urgency = if next_purchase.is_some() {
            "buy"
        } else if next_component.is_null() {
            "save"
        } else {
            "component"
        };
        let mut output = json!({
            "mode": if self.build_model.is_some() { "model" } else { "priors" },
            "champion": champion_brief(champ),
            "role": role,
            "roleLabel": self.role_labels.get(&role).cloned().unwrap_or_else(|| role.clone()),
            "side": if side == 0 { "blue" } else { "red" },
            "slot": slot + 1,
            "gold": gold,
            "minute": minute,
            "enemyProfile": profile,
            "items": items_out,
            "nextPurchase": next_purchase.unwrap_or(Value::Null),
            "buildPlan": build_plan,
            "powerSpike": {
                "slot": slot + 1,
                "n": spike.map(|value| value.n).unwrap_or(0),
                "winrate": spike.map(|value| value.winrate),
                "nearby": nearby,
            },
            "notes": notes,
        });
        if let Some(analysis) = analysis.as_ref() {
            output["gameAnalysis"] = analysis.to_json();
            output["nextComponent"] = next_component;
            output["urgency"] = json!(urgency);
        }
        Ok(output)
    }

    pub fn champion_meta(&self, role: Option<&str>) -> Result<Vec<Value>> {
        let role = role.map(|role| self.resolve_role(role)).transpose()?;
        let mut rows = Vec::new();
        for (name, data) in &self.priors.champions {
            let Some(champ) = self.resolve_champion(&ChampionRef::Str(name.clone())) else {
                continue;
            };
            let mut roles_map = Map::new();
            for (role_name, role_data) in &data.roles {
                roles_map.insert(
                    role_name.clone(),
                    json!({"n": role_data.n, "winrate": role_data.winrate}),
                );
            }
            let (pick_rate, winrate, games) = if let Some(role) = &role {
                let Some(role_data) = data.roles.get(role) else {
                    continue;
                };
                if role_data.n == 0 {
                    continue;
                }
                (
                    role_data.n as f64 / self.priors_games,
                    role_data.winrate,
                    role_data.n,
                )
            } else {
                (data.n as f64 / self.priors_games, data.winrate, data.n)
            };
            let ban_rate = data.bans as f64 / self.priors_games;
            rows.push(json!({
                "name": name,
                "key": champ.key,
                "id": champ.id,
                "icon": champ.icon,
                "tags": champ.tags,
                "damage": champ.damage,
                "games": games,
                "winrate": round(winrate, 4),
                "pickRate": round(pick_rate, 4),
                "banRate": round(ban_rate, 4),
                "presence": round((pick_rate + ban_rate).min(1.0), 4),
                "roles": roles_map,
            }));
        }
        rows.sort_by(|a, b| {
            b.get("presence")
                .and_then(Value::as_f64)
                .unwrap_or(0.0)
                .partial_cmp(&a.get("presence").and_then(Value::as_f64).unwrap_or(0.0))
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        Ok(rows)
    }

    pub fn meta_payload(&self) -> Result<Value> {
        let champion_stats = self.champion_meta(None)?;
        let stats_by_name: HashMap<String, Value> = champion_stats
            .into_iter()
            .filter_map(|row| {
                row.get("name")
                    .and_then(Value::as_str)
                    .map(|name| (name.to_string(), row.clone()))
            })
            .collect();
        let champions: Vec<Value> = self
            .champions
            .iter()
            .map(|champ| {
                let stat = stats_by_name.get(&champ.name);
                let get = |key: &str, default: Value| {
                    stat.and_then(|row| row.get(key))
                        .cloned()
                        .unwrap_or(default)
                };
                json!({
                    "id": champ.id,
                    "key": champ.key,
                    "name": champ.name,
                    "icon": champ.icon,
                    "tags": champ.tags,
                    "damage": champ.damage,
                    "info": {
                        "attack": champ.info.attack,
                        "defense": champ.info.defense,
                        "magic": champ.info.magic,
                        "difficulty": champ.info.difficulty,
                    },
                    "games": get("games", json!(0)),
                    "winrate": get("winrate", json!(0.5)),
                    "pickRate": get("pickRate", json!(0.0)),
                    "banRate": get("banRate", json!(0.0)),
                    "presence": get("presence", json!(0.0)),
                    "roles": get("roles", json!({})),
                })
            })
            .collect();
        let items: Vec<Value> = self
            .items
            .iter()
            .map(|item| {
                json!({
                    "id": item.id,
                    "name": item.name,
                    "icon": item.icon,
                    "gold": item.gold,
                    "tags": item.tags,
                    "stats": item.stats,
                    "from": item.from,
                    "into": item.into,
                    "boots": item.boots,
                })
            })
            .collect();
        Ok(json!({
            "patch": self.patch,
            "generatedAt": self.meta.get("generatedAt").cloned().unwrap_or(Value::Null),
            "iconBase": self.icon_base,
            "roles": self.roles,
            "roleLabels": self.role_labels,
            "champions": champions,
            "items": items,
            "models": {
                "draft": self.draft_model.is_some(),
                "build": self.build_model.is_some(),
                "device": "cpu",
                "notes": self.model_notes,
            },
            "dataset": {
                "games": self.summary.get("games").cloned().unwrap_or(json!(0)),
                "players": self.summary.get("players").cloned().unwrap_or(json!(0)),
                "dateRange": self.summary.get("dateRange").cloned().unwrap_or(json!([])),
                "tournaments": self.summary.get("tournaments").cloned().unwrap_or(json!(0)),
                "source": self.summary.get("source").cloned().unwrap_or(json!("")),
            },
        }))
    }

    pub fn dataset_payload(&self) -> Result<Value> {
        let mut role_baseline = Map::new();
        for (role, value) in &self.priors.role_baseline {
            role_baseline.insert(
                role.clone(),
                json!({"n": value.n, "winrate": value.winrate}),
            );
        }
        let mut item_count = Map::new();
        for (key, value) in &self.priors.item_count_winrate {
            item_count.insert(key.clone(), json!({"n": value.n, "winrate": value.winrate}));
        }
        Ok(json!({
            "summary": self.summary,
            "priors": {
                "games": self.priors.games,
                "builds": self.priors.builds,
                "dateRange": self.priors.date_range,
                "roleBaseline": role_baseline,
                "itemCountWinrate": item_count,
                "firstItems": self.priors.first_items,
            },
            "champions": self.champion_meta(None)?,
            "coverage": {
                "championsWithData": self.priors.champions.len(),
                "itemsWithData": self.priors.items.len(),
                "championPool": self.champions.len(),
                "itemPool": self.items.len(),
            },
        }))
    }

    pub fn model_payload(&self) -> Value {
        let artifact = |path: &std::path::Path, report: &Value| {
            let exists = path.is_file();
            let size = if exists {
                std::fs::metadata(path).map(|meta| meta.len()).unwrap_or(0) as f64 / 1e6
            } else {
                0.0
            };
            json!({
                "exists": exists,
                "sizeMB": round(size, 2),
                "report": report,
            })
        };
        json!({
            "draft": artifact(&self.paths.draft_model(), &self.draft_report),
            "build": artifact(&self.paths.build_model(), &self.build_report),
            "device": "cpu",
            "notes": self.model_notes,
            "patch": self.patch,
        })
    }

    fn exclude_names(
        &self,
        bans: &[&Champion],
        allies: &[Option<&Champion>],
        enemies: &[Option<&Champion>],
    ) -> Vec<String> {
        bans.iter()
            .map(|champ| champ.name.clone())
            .chain(
                allies
                    .iter()
                    .filter_map(|champ| champ.map(|c| c.name.clone())),
            )
            .chain(
                enemies
                    .iter()
                    .filter_map(|champ| champ.map(|c| c.name.clone())),
            )
            .collect()
    }
}

fn champion_brief(champ: &Champion) -> Value {
    json!({
        "id": champ.id,
        "key": champ.key,
        "name": champ.name,
        "icon": champ.icon,
        "tags": champ.tags,
        "damage": champ.damage,
    })
}

fn team_names(team: &[Option<&Champion>]) -> Vec<Value> {
    team.iter()
        .map(|champ| {
            champ
                .map(|champ| Value::String(champ.name.clone()))
                .unwrap_or(Value::Null)
        })
        .collect()
}

fn standardize(values: &[f64]) -> Vec<f64> {
    if values.len() < 2 {
        return vec![0.0; values.len()];
    }
    let mean = values.iter().sum::<f64>() / values.len() as f64;
    let variance = values
        .iter()
        .map(|value| (value - mean).powi(2))
        .sum::<f64>()
        / values.len() as f64;
    let std = variance.sqrt();
    if std < 1e-9 {
        vec![0.0; values.len()]
    } else {
        values.iter().map(|value| (value - mean) / std).collect()
    }
}

fn clamp(value: f64, min: f64, max: f64) -> f64 {
    value.max(min).min(max)
}

fn round(value: f64, decimals: i32) -> f64 {
    if !value.is_finite() {
        return value;
    }
    let factor = 10f64.powi(decimals);
    (value * factor).round() / factor
}

fn pct(value: f64, decimals: usize) -> String {
    format!("{:.*}%", decimals, value * 100.0)
}
