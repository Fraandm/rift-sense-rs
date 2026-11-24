use anyhow::{Result, bail};
use indexmap::IndexMap;
use serde::Deserialize;
use serde_json::{Map, Value, json};
use std::collections::{HashMap, HashSet};

use crate::config::Paths;
use crate::data::{Catalog, Champion, Item, MetaItem, norm_name};
use crate::mode::GameMode;
use crate::model::{BuildModel, DraftModel};
use crate::runes::RuneCatalog;
use crate::shares::{
    MIN_CLASS_ROLE_SHARE, MIN_GROUP_GAMES, MIN_OBSERVED, MIN_OWN_GAMES, MIN_ROLE_GAMES, MIN_SHARE,
    MODEL_RELIABLE_GAMES, ModePriors, ShareGroup, class_key, class_role_key, damage_profile,
    data_role, group_name, popularity_points, score_weights, shrink,
};

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
    /// Campeón ya elegido por el usuario. Se excluye de las recomendaciones
    /// aunque su hueco se envíe vacío para comparar alternativas.
    #[serde(default, rename = "myChampion")]
    pub my_champion: Option<ChampionRef>,
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
pub struct EnemyBuildContext {
    #[serde(default)]
    pub champion: String,
    #[serde(default)]
    pub items: Vec<i64>,
    #[serde(default)]
    pub kills: i64,
    #[serde(default)]
    pub deaths: i64,
    #[serde(default)]
    pub assists: i64,
    #[serde(default)]
    pub level: i64,
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
    #[serde(default)]
    pub enemy_builds: Vec<EnemyBuildContext>,
    #[serde(default)]
    pub game_mode: String,
    #[serde(default)]
    pub map_name: String,
    #[serde(default)]
    pub queue_id: Option<i64>,
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
    /// Modo explicito (`classic`, `aram`, `mayhem` o `arena`).
    #[serde(default)]
    pub mode: Option<String>,
    /// Prioriza la salida de BuildNet y el winrate historico; el contexto
    /// solo ajusta el orden. Desactivado por defecto para mantener el
    /// contrato base de `/api/build`.
    #[serde(default)]
    pub stable: bool,
    #[serde(default = "default_draft_limit")]
    pub limit: usize,
    #[serde(default = "default_true")]
    pub simulate: bool,
    /// Legendario que el usuario está construyendo. Se mantiene fijado
    /// aunque el ranking general cambie.
    #[serde(default, rename = "targetItem")]
    pub target_item: Option<ItemRef>,
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
}

#[derive(Clone, Debug, Default)]
struct RecipeProgress {
    /// Objetos que ya tienes y que se consumen al completar el legendario.
    owned: Vec<i64>,
    /// Oro ya invertido en esos objetos.
    owned_gold: i64,
}

#[derive(Clone, Debug, Default)]
struct ItemShareInfo {
    /// Participación suavizada del objeto (0..1).
    share: f64,
    /// Partidas propias del campeón con el objeto, sin suavizar.
    own_n: i64,
    avg_slot: f64,
    winrate: f64,
    /// "champ", "group" o "none".
    scope: &'static str,
}

#[derive(Clone, Debug)]
struct ItemCandidate<'a> {
    item: &'a Item,
    model_prob: f64,
    prior_n: i64,
    avg_slot: f64,
    slot_fit: f64,
    share: f64,
    scope: &'static str,
    winrate: f64,
    wr_edge: f64,
    defense: f64,
    defense_note: String,
    context_fit: f64,
    context_note: String,
    mode_fit: f64,
    mode_note: String,
    order_penalty: f64,
    price: i64,
    score: f64,
    affordable: bool,
    missing_gold: i64,
    /// Progreso de receta cuando el inventario está lleno y se recomienda
    /// completar un legendario en vez de comprar un objeto nuevo.
    recipe: Option<RecipeProgress>,
    /// Sustituye un objeto actual (botas tier 3 o mejora de soporte) en vez
    /// de ocupar una ranura nueva.
    replacement: bool,
    replaces: Option<i64>,
}

#[derive(Clone, Debug, Default)]
struct EnemyBuildDetail {
    champion: String,
    icon: String,
    items: Vec<i64>,
    gold: i64,
    armor: f64,
    magic_resist: f64,
    health: f64,
    attack_damage: f64,
    ability_power: f64,
    crit: f64,
    attack_speed: f64,
    kills: i64,
    deaths: i64,
    assists: i64,
    level: i64,
    damage: String,
    threat: String,
    healing_items: Vec<String>,
    shield_items: Vec<String>,
}

#[derive(Clone, Debug, Default)]
struct EnemyBuildSummary {
    known: i64,
    armor: f64,
    magic_resist: f64,
    health: f64,
    max_armor: f64,
    max_magic_resist: f64,
    max_health: f64,
    attack_damage: f64,
    ability_power: f64,
    crit: f64,
    attack_speed: f64,
    ad_threats: i64,
    ap_threats: i64,
    healing: Vec<String>,
    shields: Vec<String>,
    shield_priority: bool,
    details: Vec<EnemyBuildDetail>,
    notes: Vec<String>,
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
    healing_priority: bool,
    enemy_build: EnemyBuildSummary,
    notes: Vec<String>,
    mode: GameMode,
}

impl GameAnalysis {
    fn to_json(&self) -> Value {
        let enemy_details: Vec<Value> = self
            .enemy_build
            .details
            .iter()
            .map(|detail| {
                json!({
                    "champion": detail.champion,
                    "icon": detail.icon,
                    "items": detail.items,
                    "gold": detail.gold,
                    "armor": round(detail.armor, 1),
                    "magicResist": round(detail.magic_resist, 1),
                    "health": round(detail.health, 1),
                    "attackDamage": round(detail.attack_damage, 1),
                    "abilityPower": round(detail.ability_power, 1),
                    "crit": round(detail.crit, 3),
                    "attackSpeed": round(detail.attack_speed, 3),
                    "kills": detail.kills,
                    "deaths": detail.deaths,
                    "assists": detail.assists,
                    "level": detail.level,
                    "damage": detail.damage,
                    "threat": detail.threat,
                    "healingItems": detail.healing_items,
                    "shieldItems": detail.shield_items,
                })
            })
            .collect();
        let enemy_build = json!({
            "known": self.enemy_build.known,
            "armor": round(self.enemy_build.armor, 1),
            "magicResist": round(self.enemy_build.magic_resist, 1),
            "health": round(self.enemy_build.health, 1),
            "maxArmor": round(self.enemy_build.max_armor, 1),
            "maxMagicResist": round(self.enemy_build.max_magic_resist, 1),
            "maxHealth": round(self.enemy_build.max_health, 1),
            "attackDamage": round(self.enemy_build.attack_damage, 1),
            "abilityPower": round(self.enemy_build.ability_power, 1),
            "crit": round(self.enemy_build.crit, 3),
            "attackSpeed": round(self.enemy_build.attack_speed, 3),
            "adThreats": self.enemy_build.ad_threats,
            "apThreats": self.enemy_build.ap_threats,
            "healing": self.enemy_build.healing,
            "shields": self.enemy_build.shields,
            "shieldPriority": self.enemy_build.shield_priority,
            "details": enemy_details,
            "notes": self.enemy_build.notes,
        });
        json!({
            "state": self.state,
            "label": match (self.mode, self.state) {
                (GameMode::Arena, "ahead") => "Ronda fuerte",
                (GameMode::Arena, "behind") => "Ronda cuesta arriba",
                (GameMode::Arena, _) => "Ronda pareja",
                (_, "ahead") => "Vas por delante",
                (_, "behind") => "Vas por detrás",
                _ => "Partida igualada",
            },
            "phase": self.phase,
            "phaseLabel": match (self.mode, self.phase) {
                (GameMode::Arena, _) => "Ronda en curso",
                (_, "early") => "Early game",
                (_, "late") => "Late game",
                _ => "Mid game",
            },
            "gameMode": self.mode.key(),
            "modeLabel": self.mode.label(),
            "modeNotes": self.mode.notes(),
            "showTeamStats": self.mode.uses_team_stats(),
            "showCs": self.mode.uses_cs(),
            "showPhase": self.mode.uses_phase(),
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
            "healingPriority": self.healing_priority,
            "enemyBuild": enemy_build,
            "notes": self.notes,
        })
    }
}

const DEDICATED_HEALERS: &[&str] = &[
    "bard",
    "milio",
    "nami",
    "rakan",
    "senna",
    "seraphine",
    "sona",
    "soraka",
    "taric",
    "yuumi",
];
const SELF_HEALERS: &[&str] = &[
    "aatrox",
    "briar",
    "darius",
    "drmundo",
    "fiora",
    "maokai",
    "mordekaiser",
    "nasus",
    "nidalee",
    "olaf",
    "rengar",
    "sylas",
    "tahmkench",
    "tryndamere",
    "udyr",
    "vladimir",
    "volibear",
    "warwick",
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

#[allow(clippy::too_many_arguments)]
fn enemy_damage_label(
    attack_damage: f64,
    ability_power: f64,
    attack_speed: f64,
    crit: f64,
    armor: f64,
    magic_resist: f64,
    health: f64,
    tank: bool,
    support: bool,
) -> &'static str {
    let physical_score = attack_damage + attack_speed * 50.0 + crit * 80.0;
    let magic_score = ability_power;
    let tank_score = armor + magic_resist + health / 20.0;
    if tank && physical_score < 30.0 && magic_score < 60.0 {
        "Tanque"
    } else if support && physical_score < 20.0 && magic_score < 40.0 {
        "Soporte"
    } else if physical_score >= 15.0 && physical_score >= magic_score * 1.25 {
        "AD"
    } else if magic_score >= 15.0 && magic_score >= physical_score * 1.25 {
        "AP"
    } else if physical_score >= 10.0 && magic_score >= 10.0 {
        "Mixto"
    } else if tank_score >= 70.0 {
        "Tanque"
    } else if support {
        "Soporte"
    } else {
        "En construcción"
    }
}

fn enemy_threat_label(kills: i64, deaths: i64, assists: i64, gold: i64) -> &'static str {
    let kda = (kills as f64 + assists as f64 * 0.6) / (deaths as f64).max(1.0);
    if kills >= 5 || kda >= 3.5 || (kills >= 3 && assists >= 3) || gold >= 8_000 {
        "Fuerte"
    } else if kills + assists >= 2 || gold >= 2_500 {
        "Activo"
    } else {
        "Normal"
    }
}

fn item_matches_champion_damage(item: &Item, champ: &Champion) -> bool {
    let stat = |key: &str| item.stats.get(key).copied().unwrap_or(0.0);
    let tags: HashSet<&str> = item.tags.iter().map(String::as_str).collect();
    let physical = stat("FlatPhysicalDamageMod") > 0.0
        || stat("PercentAttackSpeedMod") > 0.0
        || stat("FlatCritChanceMod") > 0.0
        || stat("PercentArmorPenetrationMod") > 0.0
        || stat("FlatArmorPenetrationMod") > 0.0
        || tags.contains("Damage")
        || tags.contains("ArmorPenetration")
        || tags.contains("CriticalStrike");
    let magic = stat("FlatMagicDamageMod") > 0.0
        || stat("PercentMagicPenetrationMod") > 0.0
        || stat("FlatMagicPenetrationMod") > 0.0
        || tags.contains("SpellDamage")
        || tags.contains("MagicPenetration");
    match damage_profile(&champ.damage, champ.info.attack, champ.info.magic) {
        "AD" => !magic || physical,
        "AP" => !physical || magic,
        _ => true,
    }
}

fn situational_category(note: &str) -> &'static str {
    let note = note.to_lowercase();
    if note.contains("antiheal") {
        "antiheal"
    } else if note.contains("penetración de armadura") {
        "armor_pen"
    } else if note.contains("penetración mágica") {
        "magic_pen"
    } else if note.contains("escudos") {
        "shield"
    } else if note.contains("vida") {
        "hp"
    } else if note.contains("autoataque") {
        "auto"
    } else if note.contains("resistencia mágica") || note.contains("rm ") {
        "mr"
    } else if note.contains("armadura") {
        "armor"
    } else {
        "other"
    }
}

fn situational_priority(note: &str) -> i64 {
    match situational_category(note) {
        "antiheal" => 6,
        "armor_pen" | "magic_pen" => 5,
        "shield" | "hp" => 4,
        "auto" => 3,
        "mr" | "armor" => 2,
        _ => 1,
    }
}

fn healing_champion_tier(name: &str) -> Option<u8> {
    let key = compact_name(name);
    if DEDICATED_HEALERS.iter().any(|candidate| *candidate == key) {
        Some(2)
    } else if SELF_HEALERS.iter().any(|candidate| *candidate == key) {
        Some(1)
    } else {
        None
    }
}

fn healing_champion_points(name: &str, threat: &str) -> Option<u8> {
    healing_champion_tier(name).map(|tier| {
        if tier >= 2 || threat == "Fuerte" {
            2
        } else {
            tier
        }
    })
}

fn is_major_shield(item_name: &str) -> bool {
    let key = compact_name(item_name);
    [
        "locket",
        "seraphsembrace",
        "fimbulwinter",
        "gargoylestoneplate",
    ]
    .iter()
    .any(|needle| key.contains(needle))
}

fn shield_reduction_priority(
    shield_champions: usize,
    major_shield: bool,
    stacked_shields: bool,
) -> bool {
    shield_champions >= 2 || major_shield || stacked_shields
}

fn is_grievous_item(item: &Item) -> bool {
    let key = compact_name(&item.name);
    GRIEVOUS_ITEMS
        .iter()
        .any(|candidate| key.contains(candidate))
}

fn is_grievous_for_champion(item: &Item, champion: &Champion) -> bool {
    if !is_grievous_item(item) {
        return false;
    }
    if champion.tags.iter().any(|tag| tag == "Tank") {
        return true;
    }
    let tags: HashSet<&str> = item.tags.iter().map(String::as_str).collect();
    let key = compact_name(&item.name);
    match champion.damage.as_str() {
        "AP" => {
            tags.contains("AbilityPower")
                || tags.contains("Mana")
                || key.contains("morellonomicon")
                || key.contains("oblivionorb")
        }
        "AD" => {
            tags.contains("Damage")
                || tags.contains("AttackSpeed")
                || tags.contains("CriticalStrike")
                || key.contains("chempunk")
                || key.contains("mortalreminder")
        }
        _ => true,
    }
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
    pub extra_items: Vec<Item>,
    pub extra_item_by_id: HashMap<i64, usize>,
    pub extra_item_by_norm: HashMap<String, usize>,
    pub priors_games: f64,
    pub patch: String,
    pub icon_base: String,
    pub draft_model: Option<DraftModel>,
    pub build_model: Option<BuildModel>,
    pub runes: RuneCatalog,
    pub model_notes: Vec<String>,
    pub mode_priors: ModePriors,
    pub share_groups: HashMap<String, ShareGroup>,
    pub tag_role_shares: HashMap<String, f64>,
}

impl Engine {
    pub fn load(paths: Paths) -> Result<Self> {
        let catalog = Catalog::load(&paths)?;
        let champion_count = catalog.champions.len() as i64;
        let item_count = catalog.items.len() as i64;
        let manifest = crate::data::read_manifest(&paths.models);

        let mut model_notes = Vec::new();
        let runes = match RuneCatalog::load() {
            Ok(runes) => runes,
            Err(error) => {
                model_notes.push(format!("No se pudo cargar el catálogo de runas: {error}"));
                RuneCatalog::default()
            }
        };
        let mode_priors: ModePriors =
            match crate::config::read_json_or_default(&paths.processed.join("priors_modes.json")) {
                Ok(priors) => priors,
                Err(error) => {
                    model_notes.push(format!("No se pudieron leer los priores por modo: {error}"));
                    ModePriors::default()
                }
            };
        if mode_priors.modes.is_empty() {
            model_notes.push(
                "Sin datos de ARAM, Mayhem o Arena: sus builds usan estadísticas de Grieta."
                    .to_string(),
            );
        }
        let share_groups = build_share_groups(&catalog.champions, &catalog.priors, &mode_priors);
        let tag_role_shares = build_tag_role_shares(&catalog.champions, &catalog.priors);
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

        let mut extra_items: Vec<Item> = catalog
            .all_items_by_id
            .values()
            .filter(|item| !catalog.item_by_id.contains_key(&item.id))
            .map(item_from_meta)
            .collect();
        extra_items.sort_by_key(|item| item.id);
        let mut extra_item_by_id = HashMap::with_capacity(extra_items.len());
        let mut extra_item_by_norm = HashMap::with_capacity(extra_items.len());
        for (index, item) in extra_items.iter().enumerate() {
            extra_item_by_id.insert(item.id, index);
            extra_item_by_norm
                .entry(norm_name(&item.name))
                .or_insert(index);
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
            extra_items,
            extra_item_by_id,
            extra_item_by_norm,
            priors_games: catalog.priors_games,
            patch: catalog.patch,
            icon_base: catalog.icon_base,
            draft_model,
            build_model,
            runes,
            model_notes,
            mode_priors,
            share_groups,
            tag_role_shares,
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
            ItemRef::Int(id) => self
                .item_by_id
                .get(id)
                .map(|index| &self.items[*index])
                .or_else(|| {
                    self.extra_item_by_id
                        .get(id)
                        .map(|index| &self.extra_items[*index])
                }),
            ItemRef::Str(text) => {
                let text = text.trim();
                if text.is_empty() {
                    return None;
                }
                if let Ok(id) = text.parse::<i64>() {
                    if let Some(index) = self.item_by_id.get(&id) {
                        return Some(&self.items[*index]);
                    }
                    if let Some(index) = self.extra_item_by_id.get(&id) {
                        return Some(&self.extra_items[*index]);
                    }
                }
                self.item_by_norm
                    .get(&norm_name(text))
                    .map(|index| &self.items[*index])
                    .or_else(|| {
                        self.extra_item_by_norm
                            .get(&norm_name(text))
                            .map(|index| &self.extra_items[*index])
                    })
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
        self.team_limited(values, 5)
    }

    fn team_limited<'a>(
        &'a self,
        values: &[Option<ChampionRef>],
        limit: usize,
    ) -> Result<Vec<Option<&'a Champion>>> {
        let mut team = Vec::with_capacity(limit);
        let mut seen = HashSet::new();
        for index in 0..limit {
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

    /// Rol mas jugado del campeon en el dataset, usado en modos sin roles.
    fn primary_role(&self, champ: &Champion) -> String {
        self.priors
            .champions
            .get(&champ.name)
            .and_then(|data| {
                data.roles
                    .iter()
                    .max_by_key(|(_, value)| value.n)
                    .map(|(role, _)| role.clone())
            })
            .filter(|role| self.roles.contains(role))
            .unwrap_or_else(|| "MID".to_string())
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
        notes.sort_by_key(|note| std::cmp::Reverse(note.n));
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
        notes.sort_by_key(|note| std::cmp::Reverse(note.n));
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
        let my_selected = req
            .my_champion
            .as_ref()
            .and_then(|value| self.resolve_champion(value));
        let mut exclude = self.exclude_names(&ban_list, &ally_team, &enemy_team);
        if let Some(selected) = my_selected {
            if !exclude.contains(&selected.name) {
                exclude.push(selected.name.clone());
            }
        }

        let picked: HashSet<i64> = ally_team
            .iter()
            .chain(enemy_team.iter())
            .filter_map(|champ| champ.map(|champ| champ.key))
            .collect();
        let used: HashSet<i64> = picked
            .iter()
            .copied()
            .chain(ban_list.iter().map(|champ| champ.key))
            .chain(my_selected.map(|champ| champ.key))
            .collect();
        let my_role_idx = *self
            .role_idx
            .get(&role)
            .ok_or_else(|| anyhow::anyhow!("Rol sin indice: {role}"))?;
        let my_slot = (if side == 0 { 0 } else { 5 }) + my_role_idx - 1;

        // El pick propio puede estar como "fantasma": la UI deja su hueco
        // vacío para comparar alternativas. Para la probabilidad de victoria
        // hay que evaluarlo dentro del equipo; si no, cada jugador ve una
        // partida distinta según qué hueco tenga vacío.
        let mut evaluation_allies = ally_team.clone();
        if let Some(selected) = my_selected {
            if let Some(slot) = evaluation_allies.get_mut(my_role_idx - 1) {
                if slot.is_none() {
                    *slot = Some(selected);
                }
            }
        }
        let tensors = self.draft_inputs(side, &evaluation_allies, &enemy_team, &ban_list);
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
            let mut notes = self.model_notes.clone();
            notes.push(
                "Tu campeón ya está fijado: quítalo del slot para comparar alternativas."
                    .to_string(),
            );
            let runes = self
                .rune_recommendation(my_champ, &role, &enemy_team)
                .map(|mut value| {
                    if let Some(object) = value.as_object_mut() {
                        object.insert("focus".to_string(), json!("myChampion"));
                    }
                    value
                });
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
                "runeRecommendation": runes,
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
            .map(|(champ, _)| self.synergy_edge(&champ.name, &evaluation_allies))
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
        let mut top_champ: Option<(&Champion, f64)> = None;
        for (i, (champ, edge)) in candidates.iter().enumerate() {
            let composite = weight("model") * z_model[i] * model_confidence
                + weight("prior") * z_prior[i]
                + weight("matchup") * z_matchup[i]
                + weight("synergy") * z_synergy[i]
                + weight("meta") * z_meta[i];
            let score = clamp(50.0 + 18.0 * composite, 1.0, 99.0);
            if top_champ.is_none_or(|(_, best)| score > best) {
                top_champ = Some((*champ, score));
            }
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
        let rune_focus = my_selected.or_else(|| top_champ.map(|(champ, _)| champ));
        let runes = rune_focus
            .and_then(|champ| self.rune_recommendation(champ, &role, &enemy_team))
            .map(|mut value| {
                if let Some(object) = value.as_object_mut() {
                    object.insert(
                        "focus".to_string(),
                        json!(if my_selected.is_some() {
                            "myChampion"
                        } else {
                            "recommendation"
                        }),
                    );
                }
                value
            });

        Ok(json!({
            "mode": mode,
            "role": role,
            "roleLabel": self.role_labels.get(&role).cloned().unwrap_or_else(|| role.clone()),
            "side": if side == 0 { "blue" } else { "red" },
            "baselineWinProbability": round(base_user_p, 4),
            "runeRecommendation": runes,
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
        let my_selected = req
            .my_champion
            .as_ref()
            .and_then(|value| self.resolve_champion(value));
        let mut evaluation_allies = ally_team.clone();
        if let Some(selected) = my_selected {
            if let Some(slot) = evaluation_allies.get_mut(self.role_idx[&role] - 1) {
                if slot.is_none() {
                    *slot = Some(selected);
                }
            }
        }
        let tensors = self.draft_inputs(side, &evaluation_allies, &enemy_team, &ban_list);
        let p = self.predict_draft(&tensors, 1)?[0] as f64;
        let p_user = if side == 0 { p } else { 1.0 - p };
        let ally_edge = self.team_edge(&evaluation_allies);
        let enemy_edge = self.team_edge(&enemy_team);
        let p_prior = clamp(0.5 + 1.2 * (ally_edge - enemy_edge), 0.03, 0.97);
        let has_model = self.draft_model.is_some();
        let p_final = if has_model {
            let shrunk = 0.5 + 0.65 * (p_user - 0.5);
            clamp(0.6 * shrunk + 0.4 * p_prior, 0.03, 0.97)
        } else {
            p_prior
        };
        let my_champ = evaluation_allies
            .get(self.role_idx[&role] - 1)
            .and_then(|champ| *champ);
        let known = evaluation_allies
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

    /// Indica si un objeto ocupa una ranura de build real.
    ///
    /// Excluye consumibles, trinkets, Poro-Snax y vales de anvil que el Live
    /// Client incluye en `items` pero no son parte de la build.
    pub fn is_tracked_item(&self, item: &Item) -> bool {
        if item.gold <= 0 {
            return false;
        }
        let tags: HashSet<&str> = item.tags.iter().map(String::as_str).collect();
        if tags.contains("Consumable") || tags.contains("Trinket") {
            return false;
        }
        let name = compact_name(&item.name);
        !(name.contains("poro")
            || name.contains("anvil")
            || name.contains("voucher")
            || name.contains("statbonus")
            || name.contains("shardblade")
            || name.starts_with("legendary")
            || name.starts_with("prismatic"))
    }

    /// Un objeto final de build (legendario), no un componente ni una bota.
    fn is_legendary_item(&self, item: &Item) -> bool {
        if item.boots || is_support_item(item.id) || is_arena_starter(item) {
            return false;
        }
        if !self.is_tracked_item(item) {
            return false;
        }
        // Arena no tiene componentes y sus objetos finales pueden no traer
        // profundidad en Data Dragon: un objeto sin hijos y de 2000 oro o más
        // cuenta como legendario.
        self.all_items_by_id
            .get(&item.id)
            .map(|meta| {
                meta.into.is_empty() && (meta.depth >= 2 || meta.depth == 0) && item.gold >= 2000
            })
            .unwrap_or_else(|| item.into.is_empty() && item.gold >= 2000)
    }

    /// Indica si alguno de los objetos actuales puede formar parte de un
    /// legendario legal para este campeón, rol y modo.
    fn has_legendary_upgrade(
        &self,
        owned: &[&Item],
        mode: GameMode,
        role: &str,
        slot: usize,
    ) -> bool {
        let eligible = |item: &Item| {
            self.is_legendary_item(item)
                && self.mode_item_legal(item, mode, slot)
                && role_item_legal(item, role, mode, owned)
                && !recipe_progress(item, owned, &self.all_items_by_id)
                    .owned
                    .is_empty()
        };
        if self.items.iter().any(eligible) {
            return true;
        }
        mode != GameMode::Classic && self.extra_items.iter().any(eligible)
    }

    /// Indica si un objeto puede aparecer en las recomendaciones de este modo.
    ///
    /// En Grieta basta con que el objeto exista en el mapa de Summoner's Rift.
    /// En los modos, el mapa de Data Dragon no siempre refleja la tienda, así
    /// que también vale que el objeto aparezca comprado en partidas reales de
    /// ese modo (ver `priors_modes.json`).
    fn mode_item_legal(&self, item: &Item, mode: GameMode, slot: usize) -> bool {
        if mode == GameMode::Classic {
            return !self.map_known(item) || self.map_allows(item, "11");
        }
        let name = compact_name(&item.name);
        if name.starts_with("doran") {
            return false;
        }
        if is_mayhem_item(item.id) {
            return mode == GameMode::Mayhem;
        }
        if item.id >= 600_000 {
            return false;
        }
        if mode == GameMode::Arena && is_arena_starter(item) {
            return slot == 0;
        }
        if mode != GameMode::Arena && is_arena_starter(item) {
            return false;
        }
        let map = if mode == GameMode::Arena { "30" } else { "12" };
        if !self.map_allows(item, map) && self.observed_count(mode, item.id) < MIN_OBSERVED {
            return false;
        }
        if mode != GameMode::Arena {
            return true;
        }

        let tags: HashSet<&str> = item.tags.iter().map(String::as_str).collect();
        if tags.contains("Jungle") || tags.contains("Consumable") {
            return false;
        }
        if item.boots {
            let depth = self
                .all_items_by_id
                .get(&item.id)
                .map(|meta| meta.depth)
                .unwrap_or(0);
            return depth == 2
                || (depth == 0 && self.observed_count(mode, item.id) >= MIN_OBSERVED);
        }
        if tags.contains("Lane") {
            return false;
        }
        let final_item = self
            .all_items_by_id
            .get(&item.id)
            .is_none_or(|meta| meta.into.is_empty());
        final_item && item.gold >= 2000
    }

    fn mode_price(&self, item: &Item, mode: GameMode, _slot: usize) -> i64 {
        if mode != GameMode::Arena {
            return item.gold;
        }
        if is_arena_starter(item) || item.boots {
            return 500;
        }
        2500
    }

    fn item_mode_fit(
        &self,
        item: &Item,
        champ: &Champion,
        mode: GameMode,
        slot: usize,
    ) -> (f64, String) {
        if mode == GameMode::Arena {
            if is_arena_starter(item) && slot == 0 {
                let tags: HashSet<&str> = champ.tags.iter().map(String::as_str).collect();
                let fit = match compact_name(&item.name).as_str() {
                    "guardiansorb" if champ.damage == "AP" => 0.40,
                    "guardiansblade" | "guardianshammer" if champ.damage == "AD" => 0.34,
                    "guardianshammer" if tags.contains("Fighter") => 0.24,
                    "guardianshorn" if tags.contains("Tank") => 0.35,
                    _ => 0.0,
                };
                return (0.65 + fit, "Starter de Arena".to_string());
            }
            if item.boots && slot <= 1 {
                return (0.10, "Botas de Arena".to_string());
            }
            return (0.0, String::new());
        }
        if mode != GameMode::Mayhem || !is_mayhem_item(item.id) {
            return (0.0, String::new());
        }
        let tags: HashSet<&str> = champ.tags.iter().map(String::as_str).collect();
        let score = match item.id {
            3039 => {
                if champ.damage == "AD" {
                    0.20 + if tags.contains("Fighter") || tags.contains("Marksman") {
                        0.08
                    } else {
                        0.0
                    }
                } else {
                    -0.12
                }
            }
            123430 => {
                if champ.damage == "AP" {
                    0.20 + if tags.contains("Mage") { 0.08 } else { 0.0 }
                } else {
                    -0.10
                }
            }
            124011 => {
                if champ.damage == "AP" {
                    0.16 + if tags.contains("Fighter") || tags.contains("Marksman") {
                        0.06
                    } else {
                        0.0
                    }
                } else {
                    0.04
                }
            }
            _ => 0.0,
        };
        (score, "Exclusivo de ARAM: Mayhem".to_string())
    }

    /// Partidas propias del campeón con el objeto, en su rol (Grieta) o en el modo.
    fn own_item_games(&self, champ: &Champion, role: &str, mode: GameMode, item: &Item) -> i64 {
        if mode.is_classic() {
            return self
                .priors
                .champions
                .get(&champ.name)
                .and_then(|data| data.items.get(role))
                .and_then(|items| items.get(&item.id.to_string()))
                .map(|stat| stat.n)
                .unwrap_or(0);
        }
        self.mode_priors
            .modes
            .get(self.mode_data_key(mode))
            .and_then(|data| data.champions.get(&champ.name))
            .and_then(|stat| stat.items.get(&norm_name(&item.name)))
            .map(|stat| stat.n)
            .unwrap_or(0)
    }

    fn role_games(&self, champ: &Champion, role: &str) -> i64 {
        self.priors
            .champions
            .get(&champ.name)
            .and_then(|data| data.roles.get(role))
            .map(|stat| stat.n)
            .unwrap_or(0)
    }

    /// Clave de los datos de un modo. Mayhem usa el mismo mapa y economía que
    /// ARAM: sin muestra propia se apoya en ARAM.
    fn mode_data_key(&self, mode: GameMode) -> &'static str {
        if mode == GameMode::Mayhem && !self.mode_priors.modes.contains_key("mayhem") {
            "aram"
        } else {
            mode.key()
        }
    }

    fn observed_count(&self, mode: GameMode, item_id: i64) -> i64 {
        self.mode_priors
            .modes
            .get(self.mode_data_key(mode))
            .and_then(|data| data.observed_ids.get(&item_id.to_string()))
            .map(|observed| observed.count)
            .unwrap_or(0)
    }

    fn map_allows(&self, item: &Item, map: &str) -> bool {
        self.all_items_by_id
            .get(&item.id)
            .is_some_and(|meta| meta.maps.get(map).copied().unwrap_or(false))
    }

    fn map_known(&self, item: &Item) -> bool {
        self.all_items_by_id
            .get(&item.id)
            .is_some_and(|meta| !meta.maps.is_empty())
    }

    /// Rol con el que se leen los datos del campeón. Si casi nunca juega el
    /// rol pedido y no hay campeones parecidos en él, usa su rol habitual.
    fn effective_role(&self, champ: &Champion, requested: &str) -> (String, Option<String>) {
        let primary = self.primary_role(champ);
        let requested_games = self.role_games(champ, requested);
        let class_share = self
            .tag_role_shares
            .get(&class_role_key(&class_key(&champ.tags), requested))
            .copied()
            .unwrap_or(0.0);
        let (role, switched) = data_role(
            requested,
            requested_games,
            class_share,
            &primary,
            self.role_games(champ, &primary),
        );
        let label = |value: &str| {
            self.role_labels
                .get(value)
                .cloned()
                .unwrap_or_else(|| value.to_string())
        };
        if switched {
            let note = format!(
                "Hay pocas partidas de {} en {}: la build sigue su rol habitual, {}.",
                champ.name,
                label(requested),
                label(&role)
            );
            return (role, Some(note));
        }
        if requested_games < MIN_ROLE_GAMES {
            let note = if class_share >= MIN_CLASS_ROLE_SHARE {
                format!(
                    "Pocas partidas de {} en {}: la build se estima con campeones parecidos en ese rol.",
                    champ.name,
                    label(requested)
                )
            } else {
                format!(
                    "Sin partidas suficientes de {} en {}: recomendación orientativa.",
                    champ.name,
                    label(requested)
                )
            };
            return (role, Some(note));
        }
        (role, None)
    }

    /// Participación de un grupo para un objeto. Usa el primer grupo con
    /// partidas suficientes; el último de la lista (global) siempre sirve.
    fn share_fallback(&self, names: &[String], key: &str) -> (f64, Option<f64>) {
        for (index, name) in names.iter().enumerate() {
            let Some(group) = self.share_groups.get(name) else {
                continue;
            };
            let is_last = index + 1 == names.len();
            if group.games >= MIN_GROUP_GAMES || (is_last && group.games > 0.0) {
                return (group.share(key), group.avg_slot(key));
            }
        }
        (0.0, None)
    }

    /// Participación del objeto para el campeón en su rol (Grieta) o en su modo.
    fn item_share(
        &self,
        champ: &Champion,
        role: &str,
        mode: GameMode,
        item: &Item,
    ) -> ItemShareInfo {
        let tag = champ.tags.first().map(String::as_str).unwrap_or("");
        if mode.is_classic() {
            let key = item.id.to_string();
            let own = self
                .priors
                .champions
                .get(&champ.name)
                .and_then(|data| data.items.get(role))
                .and_then(|items| items.get(&key));
            let role_stat = self
                .priors
                .items
                .get(&key)
                .and_then(|data| data.roles.get(role));
            let games = self.role_games(champ, role) as f64;
            let own_n = own.map(|stat| stat.n).unwrap_or(0);
            let profile = damage_profile(&champ.damage, champ.info.attack, champ.info.magic);
            let mut names = Vec::new();
            if !tag.is_empty() {
                names.push(group_name(&["classic", role, tag, profile]));
            }
            names.push(group_name(&["classic", role, profile]));
            names.push(group_name(&["classic", role, "*"]));
            let (fallback, fallback_slot) = self.share_fallback(&names, &key);
            let share = shrink(own_n as f64, games, fallback);
            let (avg_slot, scope) = if own_n >= MIN_OWN_GAMES {
                (own.map(|stat| stat.avg_slot).unwrap_or(0.0), "champ")
            } else if let Some(slot) = fallback_slot {
                (slot, "group")
            } else if let Some(stat) = role_stat {
                (stat.avg_slot, "group")
            } else {
                (0.0, "none")
            };
            let winrate = match (own, role_stat) {
                (Some(stat), _) if stat.n > 0 => (stat.wins as f64 + 12.5) / (stat.n as f64 + 25.0),
                (_, Some(stat)) => stat.winrate,
                _ => 0.5,
            };
            return ItemShareInfo {
                share,
                own_n,
                avg_slot,
                winrate,
                scope,
            };
        }
        let data_key = self.mode_data_key(mode);
        if data_key != mode.key() && is_mayhem_item(item.id) {
            // Sin partidas de Mayhem, sus objetos exclusivos no tienen datos.
            // Se mantienen con una participación fija y solo si el daño del
            // campeón encaja con el objeto: Atma's es de AD; Rite of Ruin y
            // Sword of Blossoming Dawn, de AP.
            let profile = damage_profile(&champ.damage, champ.info.attack, champ.info.magic);
            let fits = if item.id == 3039 {
                profile == "AD"
            } else {
                profile == "AP"
            };
            return ItemShareInfo {
                share: if fits { MAYHEM_RULE_SHARE } else { 0.0 },
                own_n: 0,
                avg_slot: 2.0,
                winrate: 0.5,
                scope: "rule",
            };
        }
        let Some(data) = self.mode_priors.modes.get(data_key) else {
            let primary = self.primary_role(champ);
            return self.item_share(champ, &primary, GameMode::Classic, item);
        };
        let key = norm_name(&item.name);
        let champ_data = data.champions.get(&champ.name);
        let own = champ_data.and_then(|stat| stat.items.get(&key));
        let games = champ_data.map(|stat| stat.n).unwrap_or(0) as f64;
        let own_n = own.map(|stat| stat.n).unwrap_or(0);
        let profile = damage_profile(&champ.damage, champ.info.attack, champ.info.magic);
        let mut names = Vec::new();
        if !tag.is_empty() {
            names.push(group_name(&[data_key, tag, profile]));
        }
        names.push(group_name(&[data_key, profile]));
        names.push(group_name(&[data_key, "*"]));
        let (fallback, fallback_slot) = self.share_fallback(&names, &key);
        let share = shrink(own_n as f64, games, fallback);
        let global = data.items.get(&key);
        let (avg_slot, scope) = if own_n >= MIN_OBSERVED {
            (own.map(|stat| stat.avg_slot).unwrap_or(0.0), "champ")
        } else if let Some(slot) = fallback_slot {
            (slot, "group")
        } else if let Some(stat) = global {
            (stat.avg_slot, "group")
        } else {
            (0.0, "none")
        };
        let winrate = match (own, global) {
            (Some(stat), _) if stat.n > 0 => (stat.wins as f64 + 12.5) / (stat.n as f64 + 25.0),
            (_, Some(stat)) if stat.n > 0 => (stat.wins as f64 + 12.5) / (stat.n as f64 + 25.0),
            _ => 0.5,
        };
        ItemShareInfo {
            share,
            own_n,
            avg_slot,
            winrate,
            scope,
        }
    }

    fn item_typical_minute(&self, item_id: i64, role: &str) -> Option<f64> {
        let slots = self
            .timings
            .get(item_id.to_string())?
            .get(role)?
            .as_object()?;
        slots
            .values()
            .filter_map(|slot| {
                let n = slot.get("n").and_then(Value::as_i64).unwrap_or(0);
                if n < 3 {
                    return None;
                }
                slot.get("median").and_then(Value::as_f64)
            })
            .min_by(|left, right| left.partial_cmp(right).unwrap_or(std::cmp::Ordering::Equal))
    }

    fn item_order_penalty(
        &self,
        item: &Item,
        role: &str,
        minute: f64,
        slot: usize,
        mode: GameMode,
    ) -> f64 {
        if mode != GameMode::Classic || slot >= 2 {
            return 0.0;
        }
        let mut penalty = 0.0;
        if let Some(typical) = self.item_typical_minute(item.id, role) {
            if minute + 1.5 < typical {
                penalty += ((typical - minute) * 1.8).clamp(0.0, 16.0);
            }
        }
        if slot <= 1 && minute < 18.0 {
            let name = compact_name(&item.name);
            if name.contains("infinityedge") || name.contains("rabadonsdeathcap") {
                penalty += 22.0;
            }
        }
        penalty
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
        minute: f64,
        analysis: Option<&GameAnalysis>,
        mode: GameMode,
        capacity: usize,
        stable: bool,
        use_model: bool,
        allow_full_upgrades: bool,
    ) -> Result<Vec<ItemCandidate<'a>>> {
        let owned_ids: HashSet<i64> = owned.iter().map(|item| item.id).collect();
        let slot = owned.len();
        let profile = self.enemy_profile(enemy_team);
        let universe: Vec<&'a Item> = if mode.is_classic() {
            self.items.iter().collect()
        } else {
            self.items.iter().chain(self.extra_items.iter()).collect()
        };
        let mut pool: Vec<&'a Item> = Vec::new();
        let mut index_by_name: HashMap<String, usize> = HashMap::new();
        for item in universe {
            if item.boots
                || !self.is_tracked_item(item)
                || !self.mode_item_legal(item, mode, slot)
                || !role_item_legal(item, role, mode, owned)
                || !(item_matches_champion_damage(item, champ)
                    || self.own_item_games(champ, role, mode, item) >= 20)
            {
                continue;
            }
            if mode.is_classic() {
                pool.push(item);
                continue;
            }
            // En los modos, un mismo objeto puede tener varios IDs (Arena usa
            // IDs propios): se conserva el que aparece más en partidas reales.
            let key = norm_name(&item.name);
            match index_by_name.get(&key) {
                Some(&index) => {
                    if self.observed_count(mode, item.id)
                        > self.observed_count(mode, pool[index].id)
                    {
                        pool[index] = item;
                    }
                }
                None => {
                    index_by_name.insert(key, pool.len());
                    pool.push(item);
                }
            }
        }

        let mut items_tensor = [0i64; 7];
        for (i, item) in owned.iter().take(7).enumerate() {
            items_tensor[i] = item.idx;
        }
        let ally_ids = [
            ally_team
                .first()
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
                .first()
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

        // BuildNet solo se entrena con partidas de Grieta: fuera de un rol con
        // datos suficientes extrapola sin base y no se usa.
        let model_reliable =
            mode.is_classic() && self.role_games(champ, role) >= MODEL_RELIABLE_GAMES;
        let probs = if use_model && model_reliable {
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

        let owned_boots = owned.iter().find(|item| item.boots).copied();
        let current_support = owned.iter().find(|item| is_support_item(item.id)).copied();
        let regular_count = owned.iter().filter(|item| !item.boots).count();
        // Cuando el bot completa la role quest, el cliente deja de exponer las
        // botas: se mueven a la ranura dedicada (slot 8). En ese caso no hay
        // que recomendar botas ni contar una ranura normal extra.
        let bot_quest_boots =
            mode == GameMode::Classic && role == "BOT" && capacity > 6 && owned_boots.is_none();
        let mut candidates = Vec::new();
        for item in pool {
            let item_id = item.id;
            if owned_ids.contains(&item_id) {
                continue;
            }
            let is_upgrade = mode == GameMode::Classic
                && role == "MID"
                && role_quest_done(mode, role, minute)
                && owned_boots.and_then(|owned| tier3_boots_upgrade(owned.id)) == Some(item.id);
            let support_upgrade = is_support_item(item.id) && item.id != 3865;
            let support_start = mode == GameMode::Classic
                && role == "SUPPORT"
                && item.id == 3865
                && current_support.is_none();
            if item.boots && (owned_boots.is_some() || bot_quest_boots) && !is_upgrade {
                continue;
            }
            if capacity > 6 && role == "BOT" && !item.boots && regular_count >= 6 {
                continue;
            }
            if slot >= capacity && !is_upgrade && !support_upgrade && !allow_full_upgrades {
                continue;
            }
            if allow_full_upgrades
                && !is_upgrade
                && !support_upgrade
                && !self.is_legendary_item(item)
            {
                continue;
            }
            let recipe = if allow_full_upgrades && !is_upgrade && !support_upgrade {
                let progress = recipe_progress(item, owned, &self.all_items_by_id);
                if progress.owned.is_empty() {
                    continue;
                }
                Some(progress)
            } else {
                None
            };
            // Sin evidencia propia ni de un grupo parecido, el objeto no entra:
            // así un campeón en un rol raro no recibe objetos de otra posición.
            let share = self.item_share(champ, role, mode, item);
            if !is_upgrade
                && !support_upgrade
                && !support_start
                && share.share < MIN_SHARE
                && share.own_n < MIN_OWN_GAMES
            {
                continue;
            }
            let avg_slot = share.avg_slot;
            let slot_fit = if is_upgrade || support_upgrade {
                0.90
            } else if share.scope == "none" {
                0.30
            } else {
                (-0.5 * ((avg_slot - slot as f64) / 1.6).powi(2)).exp()
            };
            let wr = share.winrate;
            let wr_edge = wr - 0.5;
            let (defense, defense_note) = self.defensive_fit(item, &profile);
            let (context_fit, context_note) = analysis
                .map(|analysis| self.context_fit(item, analysis, &profile, owned, champ))
                .unwrap_or((0.0, String::new()));
            let (mut mode_fit, mut mode_note) = self.item_mode_fit(item, champ, mode, slot);
            let replaces = if is_upgrade {
                owned_boots.map(|owned| owned.id)
            } else if support_upgrade {
                current_support.map(|owned| owned.id)
            } else {
                None
            };
            if is_upgrade {
                mode_fit = 1.0;
                mode_note = "Mejora gratuita de botas (role quest MID)".to_string();
            } else if support_upgrade {
                mode_fit = 0.8;
                mode_note = "Mejora de la quest de soporte (sin coste)".to_string();
            } else if support_start {
                mode_fit = 1.0;
                mode_note = "Objeto de soporte inicial".to_string();
            }
            let model_prob = if item.idx > 0 {
                probs.get(item.idx as usize).copied().unwrap_or(0.0) as f64
            } else {
                0.0
            };
            let order_penalty = self.item_order_penalty(item, role, minute, slot, mode);
            candidates.push(ItemCandidate {
                item,
                model_prob,
                prior_n: share.own_n,
                avg_slot,
                slot_fit,
                share: share.share,
                scope: if support_start { "rule" } else { share.scope },
                winrate: wr,
                wr_edge,
                defense,
                defense_note,
                context_fit,
                context_note,
                mode_fit,
                mode_note,
                order_penalty,
                price: if let Some(recipe) = &recipe {
                    (item.gold - recipe.owned_gold).max(0)
                } else if is_upgrade || support_upgrade {
                    0
                } else {
                    self.mode_price(item, mode, slot)
                },
                score: 0.0,
                affordable: false,
                missing_gold: 0,
                recipe,
                replacement: replaces.is_some(),
                replaces,
            });
        }
        if candidates.is_empty() {
            return Ok(candidates);
        }

        // Puntuación aditiva en 0..100. Cada señal está acotada, así que un
        // objeto solo sube si tiene evidencia real; no se normaliza contra la
        // lista, que convertía diferencias de ruido en puntuaciones máximas.
        let weights = score_weights(stable, mode.is_classic());
        let pop_scale = if mode.is_classic() { 0.30 } else { 0.22 };
        for candidate in candidates.iter_mut() {
            let popularity = popularity_points(candidate.share, pop_scale);
            let winrate = (candidate.wr_edge / 0.04).clamp(-1.0, 1.0);
            let defense = candidate.defense.min(1.0);
            let context = (candidate.context_fit / 0.6).clamp(-0.2, 1.0);
            let model = (candidate.model_prob / 0.25).min(1.0);
            let mode_term = (candidate.mode_fit / 0.35).clamp(-0.4, 1.0);
            let composite = weights.popularity * popularity
                + weights.slot * candidate.slot_fit
                + weights.winrate * winrate
                + weights.defense * defense
                + weights.context * context
                + weights.model * model
                + weights.mode * mode_term;
            candidate.score = clamp(100.0 * composite - candidate.order_penalty, 1.0, 95.0);
            if mode == GameMode::Arena && is_arena_starter(candidate.item) && slot == 0 {
                candidate.score = clamp(candidate.score + 8.0, 1.0, 95.0);
            }
            if candidate.replacement {
                // La mejora de botas de mid es gratuita y siempre correcta;
                // las mejoras de soporte también sustituyen, pero compiten
                // entre las cinco opciones finales.
                if candidate.item.boots {
                    candidate.score = 96.0;
                } else {
                    candidate.score = clamp(candidate.score + 18.0, 1.0, 96.0);
                }
            } else if candidate.item.id == 3865 && role == "SUPPORT" {
                // World Atlas es obligatorio al empezar como support; debe
                // verse aunque todavía no haya oro para comprarlo.
                candidate.score = 96.0;
            }
            if let Some(recipe) = &candidate.recipe {
                // Completar un legendario con objetos que ya llevas es más
                // valioso que comprar otro objeto desde cero.
                candidate.score = clamp(
                    candidate.score + 6.0 + 3.0 * recipe.owned.len() as f64,
                    1.0,
                    96.0,
                );
            }
            candidate.affordable = gold >= candidate.price;
            candidate.missing_gold = (candidate.price - gold).max(0);
        }
        candidates.sort_by(|a, b| {
            b.score
                .partial_cmp(&a.score)
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        if mode == GameMode::Classic && role == "SUPPORT" && current_support.is_none() {
            if let Some(index) = candidates
                .iter()
                .position(|candidate| candidate.item.id == 3865)
            {
                let start = candidates.remove(index);
                candidates.insert(0, start);
            }
        }
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

    fn rune_recommendation(
        &self,
        champ: &Champion,
        role: &str,
        enemies: &[Option<&Champion>],
    ) -> Option<Value> {
        let profile = self.enemy_profile(enemies);
        let physical = profile
            .get("physical")
            .and_then(Value::as_f64)
            .unwrap_or(0.5);
        let magic = profile.get("magic").and_then(Value::as_f64).unwrap_or(0.5);
        let tanks = enemies
            .iter()
            .flatten()
            .filter(|champ| champ.tags.iter().any(|tag| tag == "Tank"))
            .count();
        let known = enemies.iter().filter(|champ| champ.is_some()).count();
        self.runes
            .recommend(champ, role, physical, magic, tanks, known)
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
            };
        }
        if champ_games >= 50 {
            return ItemPriorInfo { n: 0, wins: 0 };
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
            };
        }
        ItemPriorInfo { n: 0, wins: 0 }
    }

    fn analyze_enemy_builds(&self, builds: &[EnemyBuildContext]) -> EnemyBuildSummary {
        let mut summary = EnemyBuildSummary::default();
        for build in builds {
            let champion = self.resolve_champion(&ChampionRef::Str(build.champion.clone()));
            let mut detail = EnemyBuildDetail {
                champion: build.champion.clone(),
                icon: champion
                    .as_ref()
                    .map(|champion| champion.icon.clone())
                    .unwrap_or_default(),
                kills: build.kills,
                deaths: build.deaths,
                assists: build.assists,
                level: build.level,
                ..Default::default()
            };
            let mut healing = false;
            let mut shield = false;
            for id in &build.items {
                let Some(item) = self.resolve_item(&ItemRef::Int(*id)) else {
                    continue;
                };
                if !self.is_tracked_item(item) || item.tags.iter().any(|tag| tag == "Jungle") {
                    continue;
                }
                detail.items.push(item.id);
                detail.gold += item.gold;
                let stat = |key: &str| item.stats.get(key).copied().unwrap_or(0.0);
                detail.armor += stat("FlatArmorMod");
                detail.magic_resist += stat("FlatSpellBlockMod");
                detail.health += stat("FlatHPPoolMod");
                detail.attack_damage += stat("FlatPhysicalDamageMod");
                detail.ability_power += stat("FlatMagicDamageMod");
                detail.crit += stat("FlatCritChanceMod");
                detail.attack_speed += stat("PercentAttackSpeedMod");
                let name = compact_name(&item.name);
                if [
                    "bloodthirster",
                    "bladeoftheruinedking",
                    "ravenoushydra",
                    "sunderedsky",
                    "deathsdance",
                    "riftmaker",
                    "hextechgunblade",
                    "immortalshieldbow",
                    "mawofmalmortius",
                    "redemption",
                    "moonstonerenewer",
                    "mikaelsblessing",
                    "echoesofhelia",
                    "unendingdespair",
                ]
                .iter()
                .any(|needle| name.contains(needle))
                {
                    healing = true;
                    detail.healing_items.push(item.name.clone());
                }
                if [
                    "steraksgage",
                    "mawofmalmortius",
                    "hexdrinker",
                    "immortalshieldbow",
                    "locket",
                    "fimbulwinter",
                    "eclipse",
                    "seraphsembrace",
                    "kaenicrookern",
                    "gargoylestoneplate",
                ]
                .iter()
                .any(|needle| name.contains(needle))
                {
                    shield = true;
                    detail.shield_items.push(item.name.clone());
                }
            }
            detail.healing_items.sort();
            detail.healing_items.dedup();
            detail.shield_items.sort();
            detail.shield_items.dedup();
            if detail.items.is_empty() {
                continue;
            }

            let tags: HashSet<&str> = champion
                .as_ref()
                .map(|champion| champion.tags.iter().map(String::as_str).collect())
                .unwrap_or_default();
            let tank = tags.contains("Tank");
            let support = tags.contains("Support");
            detail.damage = enemy_damage_label(
                detail.attack_damage,
                detail.ability_power,
                detail.attack_speed,
                detail.crit,
                detail.armor,
                detail.magic_resist,
                detail.health,
                tank,
                support,
            )
            .to_string();
            detail.threat =
                enemy_threat_label(detail.kills, detail.deaths, detail.assists, detail.gold)
                    .to_string();

            summary.known += 1;
            summary.armor += detail.armor;
            summary.magic_resist += detail.magic_resist;
            summary.health += detail.health;
            summary.attack_damage += detail.attack_damage;
            summary.ability_power += detail.ability_power;
            summary.crit += detail.crit;
            summary.attack_speed += detail.attack_speed;
            summary.max_armor = summary.max_armor.max(detail.armor);
            summary.max_magic_resist = summary.max_magic_resist.max(detail.magic_resist);
            summary.max_health = summary.max_health.max(detail.health);
            if detail.damage == "AD" {
                summary.ad_threats += 1;
            }
            if detail.damage == "AP" {
                summary.ap_threats += 1;
            }
            if healing {
                summary.healing.push(detail.champion.clone());
            }
            if shield {
                summary.shields.push(detail.champion.clone());
            }
            summary.details.push(detail);
        }
        summary.healing.sort();
        summary.healing.dedup();
        summary.shields.sort();
        summary.shields.dedup();
        let shield_champions = summary
            .details
            .iter()
            .filter(|detail| !detail.shield_items.is_empty())
            .count();
        let major_shield = summary
            .details
            .iter()
            .any(|detail| detail.shield_items.iter().any(|item| is_major_shield(item)));
        let stacked_shields = summary
            .details
            .iter()
            .any(|detail| detail.shield_items.len() >= 2);
        summary.shield_priority =
            shield_reduction_priority(shield_champions, major_shield, stacked_shields);

        if summary.armor >= 180.0 {
            summary.notes.push(format!(
                "Builds rivales con mucha armadura ({:.0})",
                summary.armor
            ));
        }
        if let Some(focus) = summary
            .details
            .iter()
            .filter(|detail| detail.armor >= 80.0)
            .max_by(|left, right| {
                left.armor
                    .partial_cmp(&right.armor)
                    .unwrap_or(std::cmp::Ordering::Equal)
            })
        {
            summary.notes.push(format!(
                "{} acumula {:.0} de armadura",
                focus.champion, focus.armor
            ));
        }
        if summary.magic_resist >= 150.0 {
            summary.notes.push(format!(
                "Builds rivales con mucha resistencia mágica ({:.0})",
                summary.magic_resist
            ));
        }
        if let Some(focus) = summary
            .details
            .iter()
            .filter(|detail| detail.magic_resist >= 60.0)
            .max_by(|left, right| {
                left.magic_resist
                    .partial_cmp(&right.magic_resist)
                    .unwrap_or(std::cmp::Ordering::Equal)
            })
        {
            summary.notes.push(format!(
                "{} acumula {:.0} de resistencia mágica",
                focus.champion, focus.magic_resist
            ));
        }
        if summary.health >= 2_000.0 {
            summary.notes.push(format!(
                "Builds rivales con mucha vida ({:.0})",
                summary.health
            ));
        }
        if let Some(focus) = summary
            .details
            .iter()
            .filter(|detail| detail.health >= 1_200.0)
            .max_by(|left, right| {
                left.health
                    .partial_cmp(&right.health)
                    .unwrap_or(std::cmp::Ordering::Equal)
            })
        {
            summary.notes.push(format!(
                "{} acumula {:.0} de vida",
                focus.champion, focus.health
            ));
        }
        let autoattack: Vec<&str> = summary
            .details
            .iter()
            .filter(|detail| detail.crit >= 0.4 || detail.attack_speed >= 0.6)
            .map(|detail| detail.champion.as_str())
            .collect();
        if autoattack.len() >= 2 {
            summary.notes.push(format!(
                "Autoataque/crítico rival: {}",
                autoattack.join(", ")
            ));
        } else if summary.crit >= 0.5 || summary.attack_speed >= 0.8 {
            summary
                .notes
                .push("Builds rivales de autoataque/crítico".to_string());
        }
        if summary.ad_threats >= 3 {
            summary.notes.push(format!(
                "{} amenazas de daño AD en las builds rivales",
                summary.ad_threats
            ));
        }
        if summary.ap_threats >= 3 {
            summary.notes.push(format!(
                "{} amenazas de daño AP en las builds rivales",
                summary.ap_threats
            ));
        }
        if !summary.healing.is_empty() {
            let focus = summary
                .details
                .iter()
                .filter(|detail| !detail.healing_items.is_empty())
                .map(|detail| format!("{} ({})", detail.champion, detail.healing_items.join(" / ")))
                .collect::<Vec<_>>()
                .join(", ");
            summary.notes.push(format!("Curación por objetos: {focus}"));
        }
        if summary.shield_priority {
            let focus = summary
                .details
                .iter()
                .filter(|detail| !detail.shield_items.is_empty())
                .map(|detail| format!("{} ({})", detail.champion, detail.shield_items.join(" / ")))
                .collect::<Vec<_>>()
                .join(", ");
            summary.notes.push(format!("Escudos relevantes: {focus}"));
        }
        summary
    }

    fn analyze_game(
        &self,
        context: &GameContext,
        minute: f64,
        enemies: &[Option<&Champion>],
        mode: GameMode,
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
        let state = if mode == GameMode::Arena {
            let personal = (kda - 1.8) * 1.2
                + (context.kills - context.deaths) as f64 * 0.25
                + (context.level - 9) as f64 * 0.08;
            if personal >= 1.6 {
                "ahead"
            } else if personal <= -1.4 {
                "behind"
            } else {
                "even"
            }
        } else {
            let cs_weight = if mode == GameMode::Classic {
                80.0
            } else {
                200.0
            };
            let tempo = kill_diff as f64 * 2.0
                + level_diff * 3.0
                + cs_diff as f64 / cs_weight
                + (kda - 2.0).clamp(-2.0, 2.0) * 0.3;
            if tempo >= 3.0 {
                "ahead"
            } else if tempo <= -3.0 {
                "behind"
            } else {
                "even"
            }
        };
        let phase = if mode == GameMode::Arena {
            "arena"
        } else if minute < 14.0 {
            "early"
        } else if minute < 28.0 {
            "mid"
        } else {
            "late"
        };

        let enemy_build = self.analyze_enemy_builds(&context.enemy_builds);
        let mut healing_sources: Vec<(String, u8)> = Vec::new();
        for champion in enemies.iter().flatten() {
            let threat = enemy_build
                .details
                .iter()
                .find(|detail| detail.champion.eq_ignore_ascii_case(&champion.name))
                .map(|detail| detail.threat.as_str())
                .unwrap_or("Normal");
            if let Some(points) = healing_champion_points(&champion.name, threat) {
                healing_sources.push((champion.name.clone(), points));
            }
        }
        for name in &enemy_build.healing {
            if let Some(existing) = healing_sources
                .iter_mut()
                .find(|(champion, _)| champion.eq_ignore_ascii_case(name))
            {
                existing.1 = 2;
            } else {
                healing_sources.push((name.clone(), 2));
            }
        }
        let healing_score: u32 = healing_sources
            .iter()
            .map(|(_, points)| *points as u32)
            .sum();
        let healing_priority = healing_score >= 2;
        let mut healers = if healing_priority {
            healing_sources
                .iter()
                .map(|(name, _)| name.clone())
                .collect()
        } else {
            Vec::new()
        };
        healers.sort();
        healers.dedup();
        healers.truncate(4);

        let mut enemy_fed: Vec<String> = context
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
        if enemy_fed.is_empty() {
            enemy_fed.extend(
                enemy_build
                    .details
                    .iter()
                    .filter(|detail| detail.threat == "Fuerte")
                    .take(2)
                    .map(|detail| detail.champion.clone()),
            );
        }

        let mut notes = Vec::new();
        if mode == GameMode::Arena {
            notes.push(format!(
                "KDA {:.2} · {}/{} · nivel {}",
                kda, context.kills, context.deaths, context.level
            ));
            notes.push(format!(
                "Oro y objetos: compra completa entre rondas (nivel {})",
                context.level
            ));
        } else {
            notes.push(format!(
                "Equipos {}-{} ({:+})",
                context.team_kills, context.enemy_kills, kill_diff
            ));
            notes.push(format!(
                "{} {}-{} ({:+})",
                if mode == GameMode::Classic {
                    "CS"
                } else {
                    "CS ref."
                },
                context.team_cs,
                context.enemy_cs,
                cs_diff
            ));
            notes.push(format!(
                "Nivel medio {:.1}-{:.1} ({:+.1})",
                context.team_level, context.enemy_level, level_diff
            ));
            notes.push(format!("KDA {:.1} · {:.1} CS/min", kda, cs_per_min));
        }
        if !healers.is_empty() {
            notes.push(format!("Curación enemiga: {}", healers.join(", ")));
        }
        if !enemy_fed.is_empty() {
            notes.push(format!("Rivales fuertes: {}", enemy_fed.join(", ")));
        }
        notes.extend(enemy_build.notes.iter().cloned());

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
            healing_priority,
            enemy_build,
            notes,
            mode,
        })
    }

    fn context_fit(
        &self,
        item: &Item,
        analysis: &GameAnalysis,
        profile: &Value,
        owned: &[&Item],
        champ: &Champion,
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
        let mut notes: Vec<String> = Vec::new();

        if analysis.state == "behind" {
            if defensive {
                score += 0.20;
                notes.push("Vas por detrás: prioriza resistencias".to_string());
            }
            if offensive {
                score -= 0.05;
            }
        } else if analysis.state == "ahead" {
            if offensive {
                score += 0.10;
                notes.push("Ventaja: acelera tu spike ofensivo".to_string());
            }
            if defensive {
                score -= 0.03;
            }
        }

        if analysis.healing_priority
            && !analysis.healers.is_empty()
            && is_grievous_for_champion(item, champ)
            && !owned.iter().any(|owned| is_grievous_item(owned))
        {
            score += 0.35;
            let focus = analysis
                .healers
                .iter()
                .take(2)
                .cloned()
                .collect::<Vec<_>>()
                .join(" y ");
            if focus.is_empty() {
                notes.push("Antiheal contra la curación rival".to_string());
            } else {
                notes.push(format!("Antiheal contra la curación de {focus}"));
            }
        }

        if !analysis.enemy_fed.is_empty() {
            if physical >= 0.55 && armor {
                score += 0.10;
                notes.push("Armadura contra el rival fuerte".to_string());
            }
            if magic >= 0.55 && magic_resist {
                score += 0.10;
                notes.push("Resistencia mágica contra el rival fuerte".to_string());
            }
        }

        if analysis.phase == "late" && item.gold >= 2800 {
            score += 0.04;
        }

        let armor_pen = tags.contains("ArmorPenetration")
            || stat("PercentArmorPenetrationMod") > 0.0
            || stat("FlatArmorPenetrationMod") > 0.0;
        let magic_pen = tags.contains("MagicPenetration")
            || stat("PercentMagicPenetrationMod") > 0.0
            || stat("FlatMagicPenetrationMod") > 0.0;
        let name = compact_name(&item.name);
        let hp_shred = [
            "liandry",
            "demonic",
            "bladeoftheruinedking",
            "kraken",
            "blackcleaver",
        ]
        .iter()
        .any(|needle| name.contains(needle));
        let shield_reduction = name.contains("serpentsfang");
        let enemy_build = &analysis.enemy_build;
        let armor_focus = enemy_build
            .details
            .iter()
            .filter(|detail| detail.armor >= 80.0)
            .max_by(|left, right| {
                left.armor
                    .partial_cmp(&right.armor)
                    .unwrap_or(std::cmp::Ordering::Equal)
            });
        let mr_focus = enemy_build
            .details
            .iter()
            .filter(|detail| detail.magic_resist >= 60.0)
            .max_by(|left, right| {
                left.magic_resist
                    .partial_cmp(&right.magic_resist)
                    .unwrap_or(std::cmp::Ordering::Equal)
            });
        let hp_focus = enemy_build
            .details
            .iter()
            .filter(|detail| detail.health >= 1_200.0)
            .max_by(|left, right| {
                left.health
                    .partial_cmp(&right.health)
                    .unwrap_or(std::cmp::Ordering::Equal)
            });
        if (enemy_build.armor >= 150.0 || enemy_build.max_armor >= 80.0)
            && armor_pen
            && (champ.damage == "AD" || champ.tags.iter().any(|tag| tag == "Marksman"))
        {
            score += 0.12;
            match armor_focus {
                Some(focus) => {
                    notes.push(format!("Penetración de armadura contra {}", focus.champion))
                }
                None => notes.push("Penetración de armadura contra las builds rivales".to_string()),
            }
        }
        if (enemy_build.magic_resist >= 120.0 || enemy_build.max_magic_resist >= 60.0)
            && magic_pen
            && (champ.damage == "AP" || champ.tags.iter().any(|tag| tag == "Mage"))
        {
            score += 0.12;
            match mr_focus {
                Some(focus) => notes.push(format!(
                    "Penetración mágica contra la RM de {}",
                    focus.champion
                )),
                None => notes.push("Penetración mágica contra las builds rivales".to_string()),
            }
        }
        if (enemy_build.health >= 1_500.0 || enemy_build.max_health >= 1_200.0) && hp_shred {
            score += 0.10;
            match hp_focus {
                Some(focus) => notes.push(format!(
                    "Daño porcentual contra la vida de {}",
                    focus.champion
                )),
                None => notes.push("Daño porcentual contra la vida rival".to_string()),
            }
        }
        let ad_champion = champ.damage == "AD"
            || champ.tags.iter().any(|tag| tag == "Marksman")
            || (champ.damage != "AP"
                && champ
                    .tags
                    .iter()
                    .any(|tag| matches!(tag.as_str(), "Assassin" | "Fighter")));
        if enemy_build.shield_priority && shield_reduction && ad_champion {
            score += 0.15;
            notes.push(format!(
                "Reducción de escudos contra {}",
                enemy_build.shields.join(", ")
            ));
        }
        let auto_threats: Vec<&str> = enemy_build
            .details
            .iter()
            .filter(|detail| detail.crit >= 0.4 || detail.attack_speed >= 0.6)
            .map(|detail| detail.champion.as_str())
            .collect();
        if (enemy_build.crit >= 0.4 || enemy_build.attack_speed >= 0.6 || auto_threats.len() >= 2)
            && armor
        {
            score += 0.08;
            if auto_threats.is_empty() {
                notes.push("Armadura contra el daño de autoataque rival".to_string());
            } else {
                notes.push(format!(
                    "Armadura contra el autoataque de {}",
                    auto_threats
                        .iter()
                        .take(2)
                        .copied()
                        .collect::<Vec<_>>()
                        .join(" y ")
                ));
            }
        }

        (clamp(score, -0.1, 0.6), notes.join(" · "))
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

    fn item_brief(&self, id: i64) -> Value {
        if let Some(item) = self.all_items_by_id.get(&id) {
            return json!({
                "id": id,
                "name": item.name,
                "icon": item.icon,
                "gold": item.gold,
            });
        }
        if let Some(index) = self.item_by_id.get(&id) {
            let item = &self.items[*index];
            return json!({
                "id": id,
                "name": item.name,
                "icon": item.icon,
                "gold": item.gold,
            });
        }
        if let Some(index) = self.extra_item_by_id.get(&id) {
            let item = &self.extra_items[*index];
            return json!({
                "id": id,
                "name": item.name,
                "icon": item.icon,
                "gold": item.gold,
            });
        }
        json!({"id": id, "name": format!("Objeto {id}"), "icon": "", "gold": 0})
    }

    fn recipe_components(&self, progress: &RecipeProgress) -> Vec<Value> {
        progress
            .owned
            .iter()
            .map(|id| self.item_brief(*id))
            .collect()
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

    fn format_item(
        &self,
        candidate: &ItemCandidate,
        gold: i64,
        slot: usize,
        role: &str,
        mode: GameMode,
    ) -> Value {
        let item = candidate.item;
        let prior_n = candidate.prior_n;
        let avg_slot = candidate.avg_slot;
        let is_upgrade = candidate.recipe.is_some();
        let display_slot = if is_upgrade || candidate.replacement {
            slot.max(1)
        } else {
            slot + 1
        };
        let typical_minute = self
            .timings
            .get(item.id.to_string())
            .and_then(|value| value.get(role))
            .and_then(|value| value.get(slot.to_string()))
            .and_then(|value| value.get("median"))
            .and_then(Value::as_f64)
            .filter(|value| *value != 0.0);
        let mut reasons = Vec::new();
        if candidate.scope == "group" {
            reasons.push("Estimado con campeones parecidos: pocas partidas propias".to_string());
        }
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
        if !candidate.mode_note.is_empty() {
            reasons.push(candidate.mode_note.clone());
        }
        if candidate.order_penalty > 0.0 {
            reasons.push("Mejor como objeto posterior".to_string());
        }
        if mode == GameMode::Arena {
            reasons.push("Precio de Arena: objeto completo".to_string());
        }
        if candidate.affordable {
            reasons.push(if is_upgrade {
                "Puedes completarlo ahora".to_string()
            } else {
                "Puedes comprarlo ahora".to_string()
            });
        } else {
            reasons.push(format!("Te faltan {} de oro", candidate.missing_gold));
        }
        if let Some(recipe) = &candidate.recipe {
            reasons.push(format!(
                "Reutiliza {} objeto(s) de tu inventario",
                recipe.owned.len()
            ));
            let freed = recipe.owned.len().saturating_sub(1);
            if freed > 0 {
                reasons.push(format!("Libera {freed} hueco(s) al completarlo"));
            }
        }
        let components = if mode.uses_components() && !candidate.replacement && !is_upgrade {
            self.components(item, gold)
        } else {
            Vec::new()
        };
        let owned_components: Vec<Value> = candidate
            .recipe
            .as_ref()
            .map(|recipe| self.recipe_components(recipe))
            .unwrap_or_default();
        let consumes: Vec<i64> = candidate
            .recipe
            .as_ref()
            .map(|recipe| recipe.owned.clone())
            .unwrap_or_default();
        let freed_slots = candidate
            .recipe
            .as_ref()
            .map(|recipe| recipe.owned.len().saturating_sub(1))
            .unwrap_or(0);
        if !candidate.affordable && mode.uses_components() {
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
            "gold": candidate.price,
            "fullGold": item.gold,
            "upgrade": is_upgrade,
            "ownedComponents": owned_components,
            "consumes": consumes,
            "freesSlots": freed_slots,
            "tags": item.tags,
            "score": round(candidate.score, 1),
            "affordable": candidate.affordable,
            "missingGold": candidate.missing_gold,
            "avgSlot": if prior_n > 0 { json!(round(avg_slot + 1.0, 2)) } else { Value::Null },
            "typicalMinute": json!(typical_minute.map(|value| round(value, 2))),
            "priorGames": prior_n,
            "share": round(candidate.share, 4),
            "scope": candidate.scope,
            "winrate": round(candidate.winrate, 4),
            "modelProb": round(candidate.model_prob, 5),
            "contextFit": round(candidate.context_fit, 3),
            "contextNote": candidate.context_note,
            "components": components,
            "reasons": reasons,
            "slot": display_slot,
            "replacement": candidate.replacement,
            "replaces": candidate.replaces,
        })
    }

    fn legendary_cost(&self, candidate: &ItemCandidate, owned: &[&Item], mode: GameMode) -> i64 {
        let recipe = candidate
            .recipe
            .clone()
            .unwrap_or_else(|| recipe_progress(candidate.item, owned, &self.all_items_by_id));
        if mode.uses_components() {
            (candidate.item.gold - recipe.owned_gold).max(0)
        } else {
            candidate.item.gold
        }
    }

    fn boots_score(
        &self,
        item: &Item,
        champ: &Champion,
        profile: &Value,
        role: &str,
        enemy_build: Option<&EnemyBuildSummary>,
    ) -> (f64, Vec<String>) {
        let tags: HashSet<&str> = champ.tags.iter().map(String::as_str).collect();
        let item_tags: HashSet<&str> = item.tags.iter().map(String::as_str).collect();
        let tank = tags.contains("Tank");
        let fighter = tags.contains("Fighter");
        let support = tags.contains("Support");
        let ap = !tank && (champ.damage == "AP" || tags.contains("Mage"));
        let ad = !tank
            && !ap
            && (champ.damage == "AD"
                || tags.contains("Marksman")
                || tags.contains("Assassin")
                || fighter);
        let physical = profile
            .get("physical")
            .and_then(Value::as_f64)
            .unwrap_or(0.5);
        let magic = profile.get("magic").and_then(Value::as_f64).unwrap_or(0.5);
        let mut score = 50.0;
        let mut reasons = Vec::new();
        match item.id {
            3020 => {
                if tank {
                    score = 55.0;
                } else if ap {
                    score = 90.0;
                    reasons.push("Penetración mágica para tu daño AP".to_string());
                } else if support {
                    score = 70.0;
                } else {
                    score = 54.0;
                }
                if let Some(enemy) = enemy_build {
                    if ap && enemy.max_magic_resist >= 60.0 {
                        score += 4.0;
                        reasons.push("Penetración mágica contra la RM rival".to_string());
                    }
                }
            }
            3006 => {
                if tags.contains("Marksman") {
                    score = 90.0;
                    reasons.push("Velocidad de ataque para tu daño AD".to_string());
                } else if ad {
                    score = 70.0;
                    reasons.push(
                        "Velocidad de ataque, aunque no es tu prioridad principal".to_string(),
                    );
                } else {
                    score = 52.0;
                }
            }
            3047 => {
                if physical >= 0.55 {
                    score = 88.0;
                    reasons.push(format!(
                        "Armadura contra una composición {:.0}% física",
                        physical * 100.0
                    ));
                } else if tank {
                    score = 78.0;
                } else {
                    score = 55.0;
                }
                if let Some(enemy) = enemy_build {
                    let auto_threats = enemy
                        .details
                        .iter()
                        .filter(|detail| detail.crit >= 0.4 || detail.attack_speed >= 0.6)
                        .count();
                    if enemy.crit >= 0.4 || enemy.attack_speed >= 0.6 || auto_threats >= 2 {
                        score += 8.0;
                        reasons
                            .push("Armadura contra las builds de autoataque rivales".to_string());
                    }
                }
            }
            3111 => {
                if magic >= 0.55 {
                    score = 88.0;
                    reasons.push(format!(
                        "Resistencia mágica contra una composición {:.0}% mágica",
                        magic * 100.0
                    ));
                } else if tank {
                    score = 76.0;
                } else {
                    score = 54.0;
                }
                if let Some(enemy) = enemy_build {
                    if enemy.ap_threats >= 2 {
                        score += 8.0;
                        reasons.push(format!(
                            "Resistencia mágica contra {} amenazas AP",
                            enemy.ap_threats
                        ));
                    }
                }
            }
            3158 => {
                if support {
                    score = 92.0;
                } else if tank {
                    score = 60.0;
                } else if tags.contains("Assassin") || fighter {
                    score = 82.0;
                } else if ap {
                    score = 74.0;
                } else {
                    score = 64.0;
                }
                reasons.push("Aceleración de habilidades para tu rol".to_string());
            }
            3009 => {
                score = 60.0;
                reasons.push("Velocidad de movimiento para recomponer".to_string());
            }
            _ => {
                if item_tags.contains("Boots") {
                    score = 55.0;
                }
            }
        }
        let prior = self.item_prior(&champ.name, role, item.id);
        let champ_games = self
            .priors
            .champions
            .get(&champ.name)
            .and_then(|data| data.roles.get(role))
            .map(|data| data.n)
            .unwrap_or(0);
        if prior.n >= 20 {
            let winrate = (prior.wins as f64 + 12.5) / (prior.n as f64 + 25.0);
            let edge = winrate - 0.5;
            score += (edge * 24.0).clamp(-4.0, 4.0);
            if edge > 0.005 {
                reasons.push(format!(
                    "Winrate {} en {} con estas botas",
                    pct(winrate, 1),
                    role
                ));
            }
            if champ_games >= 100 {
                let share = prior.n as f64 / champ_games as f64;
                score += (share * 80.0).clamp(0.0, 24.0);
                if share >= 0.10 {
                    reasons.push(format!("Uso histórico {} en {}", pct(share, 0), role));
                }
            }
        } else if champ_games >= 200 {
            score -= 10.0;
            reasons.push("Sin muestra histórica de estas botas".to_string());
        }
        if reasons.is_empty() {
            reasons.push("Botas estándar para tu rol".to_string());
        }
        (score.max(1.0), reasons)
    }

    #[allow(clippy::too_many_arguments)]
    fn boots_recommendation(
        &self,
        champ: &Champion,
        role: &str,
        mode: GameMode,
        slot: usize,
        minute: f64,
        gold: i64,
        owned: &[&Item],
        profile: &Value,
        enemy_build: Option<&EnemyBuildSummary>,
    ) -> Value {
        let owned_boots = owned.iter().find(|item| item.boots).copied();
        if let Some(owned) = owned_boots.filter(|item| item.id != 1001) {
            let upgrade_id = if mode == GameMode::Classic
                && role == "MID"
                && role_quest_done(mode, role, minute)
            {
                tier3_boots_upgrade(owned.id)
            } else {
                None
            };
            if let Some(upgrade_id) = upgrade_id {
                if let Some(upgrade) = self
                    .items
                    .iter()
                    .chain(self.extra_items.iter())
                    .find(|item| item.id == upgrade_id)
                {
                    return json!({
                        "id": upgrade.id,
                        "name": upgrade.name,
                        "icon": upgrade.icon,
                        "gold": 0,
                        "price": 0,
                        "owned": false,
                        "upgrade": true,
                        "affordable": true,
                        "missingGold": 0,
                        "score": 96.0,
                        "reasons": ["Mejora gratuita de botas por la role quest"],
                    });
                }
            }
            return json!({
                "id": owned.id,
                "name": owned.name,
                "icon": owned.icon,
                "gold": owned.gold,
                "price": 0,
                "owned": true,
                "upgrade": false,
                "affordable": false,
                "missingGold": 0,
                "score": 90.0,
                "reasons": ["Ya llevas botas"],
            });
        }

        let mut best: Option<(&Item, f64, Vec<String>)> = None;
        for item in self.items.iter().chain(self.extra_items.iter()) {
            if !item.boots
                || TIER3_BOOTS.iter().any(|(_, tier3)| *tier3 == item.id)
                || !self.mode_item_legal(item, mode, slot)
                || !role_item_legal(item, role, mode, owned)
            {
                continue;
            }
            let (score, reasons) = if mode == GameMode::Arena {
                // Arena usa IDs propios para las botas: se puntúan con la regla
                // de su equivalente de Grieta, que tiene el mismo nombre.
                let mut twin = item.clone();
                if let Some(index) = self.item_by_norm.get(&norm_name(&item.name)) {
                    twin.id = self.items[*index].id;
                }
                self.boots_score(&twin, champ, profile, role, enemy_build)
            } else {
                self.boots_score(item, champ, profile, role, enemy_build)
            };
            if best
                .as_ref()
                .is_none_or(|(_, best_score, _)| score > *best_score)
            {
                best = Some((item, score, reasons));
            }
        }
        let Some((item, score, reasons)) = best else {
            return Value::Null;
        };
        let basic_boots = owned_boots.filter(|owned| owned.id == 1001);
        let price = (self.mode_price(item, mode, slot)
            - basic_boots
                .filter(|owned| item.from.contains(&owned.id))
                .map_or(0, |owned| owned.gold))
        .max(0);
        json!({
            "id": item.id,
            "name": item.name,
            "icon": item.icon,
            "gold": price,
            "price": price,
            "owned": false,
            "upgrade": basic_boots.is_some_and(|owned| item.from.contains(&owned.id)),
            "affordable": gold >= price,
            "missingGold": (price - gold).max(0),
            "score": round(score.min(96.0), 1),
            "reasons": reasons,
        })
    }

    fn format_legendary_item(
        &self,
        item: &Item,
        score: f64,
        gold: i64,
        owned: &[&Item],
        mode: GameMode,
    ) -> Value {
        let recipe = recipe_progress(item, owned, &self.all_items_by_id);
        let cost = if mode.uses_components() {
            (item.gold - recipe.owned_gold).max(0)
        } else {
            item.gold
        };
        let owned_components = self.recipe_components(&recipe);
        let freed_slots = recipe.owned.len().saturating_sub(1);
        json!({
            "id": item.id,
            "name": item.name,
            "icon": item.icon,
            "gold": item.gold,
            "cost": cost,
            "missingGold": (cost - gold).max(0),
            "ownedGold": recipe.owned_gold,
            "affordable": gold >= cost,
            "score": round(score, 1),
            "ownedComponents": owned_components,
            "freesSlots": freed_slots,
            "needsSlot": recipe.owned.is_empty(),
        })
    }

    fn format_legendary(
        &self,
        candidate: &ItemCandidate,
        gold: i64,
        owned: &[&Item],
        mode: GameMode,
    ) -> Value {
        self.format_legendary_item(candidate.item, candidate.score, gold, owned, mode)
    }

    fn next_component(
        &self,
        candidates: &[&ItemCandidate],
        gold: i64,
        owned_ids: &HashSet<i64>,
        mode: GameMode,
    ) -> Value {
        if !mode.uses_components() {
            return Value::Null;
        }
        for candidate in candidates.iter().take(5) {
            for component in self.components(candidate.item, gold) {
                let affordable = component
                    .get("affordable")
                    .and_then(Value::as_bool)
                    .unwrap_or(false);
                let id = component.get("id").and_then(Value::as_i64).unwrap_or(0);
                let legal = id > 0
                    && self
                        .resolve_item(&ItemRef::Int(id))
                        .is_some_and(|item| self.mode_item_legal(item, mode, 0));
                if affordable && legal && !owned_ids.contains(&id) {
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
        let mode = GameMode::resolve(
            req.mode.as_deref(),
            req.game
                .as_ref()
                .map(|game| game.game_mode.as_str())
                .filter(|value| !value.is_empty()),
            req.game
                .as_ref()
                .map(|game| game.map_name.as_str())
                .filter(|value| !value.is_empty()),
            req.game.as_ref().and_then(|game| game.queue_id),
        );
        let (role, role_note) = if mode.uses_roles() {
            let requested = self.resolve_role(&req.role)?;
            self.effective_role(champ, &requested)
        } else {
            (self.primary_role(champ), None)
        };
        let side = self.resolve_side(&req.side)?;
        let ally_team = self.team(&req.allies)?;
        let enemy_team = self.team_limited(&req.enemies, mode.max_enemies())?;
        let gold = req.gold.clamp(0, 100_000);
        let minute = req.minute.clamp(0.0, 120.0);
        let limit = req.limit.clamp(1, 25);
        let stable = req.stable;

        let mut owned_items: Vec<&Item> = Vec::new();
        for value in req.owned.iter().take(8) {
            if let Some(item) = self.resolve_item(value) {
                let counted = match item.id {
                    id if is_support_item(id) => mode == GameMode::Classic && role == "SUPPORT",
                    id if is_jungle_pet(id) => role == "JUNGLE",
                    _ => true,
                };
                if counted && self.is_tracked_item(item) {
                    owned_items.push(item);
                }
            }
        }
        let capacity = build_capacity(mode, &role, minute, owned_items.len());
        let slot = owned_items.len();
        let requested_target = req
            .target_item
            .as_ref()
            .and_then(|value| self.resolve_item(value));
        let target_owned = requested_target
            .is_some_and(|item| owned_items.iter().any(|owned| owned.id == item.id));
        let target_active = requested_target.filter(|item| {
            !target_owned
                && self.is_legendary_item(item)
                && self.mode_item_legal(item, mode, slot)
                && role_item_legal(item, &role, mode, &owned_items)
        });
        let target_status = match (
            req.target_item.is_some(),
            target_active.is_some(),
            target_owned,
        ) {
            (false, _, _) => "none",
            (true, true, _) => "active",
            (true, false, true) => "completed",
            _ => "invalid",
        };
        let regular_count = owned_items.iter().filter(|item| !item.boots).count();
        let profile = self.enemy_profile(&enemy_team);
        let analysis = req
            .game
            .as_ref()
            .and_then(|game| self.analyze_game(game, minute, &enemy_team, mode));
        let boots_recommendation = self.boots_recommendation(
            champ,
            &role,
            mode,
            slot,
            minute,
            gold,
            &owned_items,
            &profile,
            analysis.as_ref().map(|analysis| &analysis.enemy_build),
        );
        let has_boots_upgrade = mode == GameMode::Classic
            && role == "MID"
            && role_quest_done(mode, &role, minute)
            && owned_items
                .iter()
                .any(|item| item.boots && tier3_boots_upgrade(item.id).is_some());
        let has_support_upgrade = mode == GameMode::Classic
            && role == "SUPPORT"
            && owned_items
                .iter()
                .any(|item| matches!(item.id, 3865..=3867));
        let bot_build_full =
            mode == GameMode::Classic && role == "BOT" && capacity > 6 && regular_count >= 6;
        let full_inventory = slot >= capacity || bot_build_full;
        // Orientación conservadora, no una optimización aprendida: mantener
        // los iniciales hasta necesitar realmente su ranura.
        let sell_starter = if mode == GameMode::Classic {
            owned_items
                .iter()
                .find(|item| matches!(item.id, 1054..=1056))
                .map(|item| json!({
                    "id": item.id,
                    "name": item.name,
                    "recommended": full_inventory,
                    "reason": if full_inventory {
                        "Considera venderlo al volver a base si necesitas su ranura para una compra útil; conserva el objeto si puedes completar una receta sin vender."
                    } else {
                        "Consérvalo mientras no necesites la ranura; véndelo al volver a base para completar una mejora útil."
                    },
                    "method": "heuristic"
                }))
                .unwrap_or(Value::Null)
        } else {
            Value::Null
        };
        let has_upgrade_material = self.has_legendary_upgrade(&owned_items, mode, &role, slot);
        // Con el inventario lleno de componentes no damos la build por
        // terminada: buscamos un legendario que reutilice esos componentes.
        let full_upgrades = full_inventory
            && mode.uses_components()
            && (has_upgrade_material || target_active.is_some());
        if full_inventory && !has_boots_upgrade && !has_support_upgrade && !full_upgrades {
            let complete_note = match (mode, role.as_str(), capacity) {
                (GameMode::Arena, _, _) => "Build completa: 6 objetos de Arena.".to_string(),
                (_, "BOT", 7) => {
                    "Build completa: 6 objetos + botas en la ranura de role quest.".to_string()
                }
                _ => "Build completa: 6 objetos. Considera el trinket y consumibles.".to_string(),
            };
            let mut complete = json!({
                "mode": "complete",
                "champion": champion_brief(champ),
                "role": role,
                "slot": capacity,
                "slotCap": capacity,
                "fullInventory": true,
                "targetStatus": target_status,
                "bootsRecommendation": boots_recommendation,
                "situationalItems": [],
                "enemyProfile": profile,
                "items": [],
                "buildPlan": [],
                "sellStarter": sell_starter,
                "notes": [if sell_starter.is_null() { complete_note } else {
                    "Inventario lleno con objeto inicial: revisa si conviene venderlo para liberar una ranura; no es necesariamente una build final.".to_string()
                }],
            });
            if !mode.is_classic() {
                complete["gameMode"] = json!(mode.key());
                complete["modeLabel"] = json!(mode.label());
                complete["modeNotes"] = json!(mode.notes());
            }
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
            mode,
            capacity,
            stable,
            true,
            full_upgrades,
        )?;
        let items_out: Vec<Value> = candidates
            .iter()
            .take(limit)
            .map(|candidate| self.format_item(candidate, gold, slot, &role, mode))
            .collect();
        let focus: Vec<&ItemCandidate> = if stable {
            candidates.iter().collect()
        } else if let Some(analysis) = analysis.as_ref() {
            let mut preferred = if !analysis.healers.is_empty()
                && !owned_items.iter().any(|item| is_grievous_item(item))
            {
                candidates
                    .iter()
                    .filter(|candidate| is_grievous_for_champion(candidate.item, champ))
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
        let next_candidate = focus
            .iter()
            .copied()
            .find(|candidate| gold >= candidate.price)
            .or_else(|| candidates.iter().find(|candidate| gold >= candidate.price));
        let next_purchase =
            next_candidate.map(|candidate| self.format_item(candidate, gold, slot, &role, mode));
        let antiheal_source =
            next_candidate.is_some_and(|candidate| is_grievous_for_champion(candidate.item, champ));
        let legendary_pool: Vec<&ItemCandidate> = candidates
            .iter()
            .filter(|candidate| self.is_legendary_item(candidate.item))
            .collect();
        let target_legendary = legendary_pool
            .iter()
            .copied()
            .find(|candidate| gold >= self.legendary_cost(candidate, &owned_items, mode))
            .or_else(|| legendary_pool.first().copied());
        let (next_legendary, legendary_options): (Option<Value>, Vec<Value>) = if let Some(target) =
            target_active
        {
            let score = legendary_pool
                .iter()
                .find(|candidate| candidate.item.id == target.id)
                .map(|candidate| candidate.score)
                .unwrap_or(0.0);
            let target_json = self.format_legendary_item(target, score, gold, &owned_items, mode);
            let options = legendary_pool
                .iter()
                .copied()
                .filter(|candidate| candidate.item.id != target.id)
                .take(2)
                .map(|candidate| self.format_legendary(candidate, gold, &owned_items, mode))
                .collect();
            (Some(target_json), options)
        } else {
            let next = target_legendary
                .map(|candidate| self.format_legendary(candidate, gold, &owned_items, mode));
            let options = legendary_pool
                .iter()
                .copied()
                .filter(|candidate| {
                    target_legendary.is_none_or(|target| candidate.item.id != target.item.id)
                })
                .take(2)
                .map(|candidate| self.format_legendary(candidate, gold, &owned_items, mode))
                .collect();
            (next, options)
        };
        let next_candidate_id = next_candidate.map(|candidate| candidate.item.id);
        let mut situational: Vec<&ItemCandidate> = candidates
            .iter()
            .filter(|candidate| {
                if candidate.item.boots
                    || candidate.context_fit < 0.05
                    || Some(candidate.item.id) == next_candidate_id
                    || !item_matches_champion_damage(candidate.item, champ)
                {
                    return false;
                }
                let note = candidate.context_note.to_lowercase();
                [
                    "rival",
                    "curación",
                    "curacion",
                    "autoataque",
                    "escudo",
                    "vida",
                    "armadura",
                    "rm ",
                    "penetración",
                    "penetracion",
                ]
                .iter()
                .any(|needle| note.contains(needle))
            })
            .collect();
        situational.sort_by(|left, right| {
            situational_priority(&right.context_note)
                .cmp(&situational_priority(&left.context_note))
                .then_with(|| {
                    right
                        .context_fit
                        .partial_cmp(&left.context_fit)
                        .unwrap_or(std::cmp::Ordering::Equal)
                })
                .then_with(|| {
                    right
                        .score
                        .partial_cmp(&left.score)
                        .unwrap_or(std::cmp::Ordering::Equal)
                })
        });
        let mut selected: Vec<&ItemCandidate> = Vec::new();
        let mut categories: HashSet<&'static str> = HashSet::new();
        for candidate in &situational {
            if !categories.insert(situational_category(&candidate.context_note)) {
                continue;
            }
            selected.push(*candidate);
            if selected.len() >= 2 {
                break;
            }
        }
        let situational_items: Vec<Value> = selected
            .iter()
            .map(|candidate| self.format_item(candidate, gold, slot, &role, mode))
            .collect();

        let mut build_plan = Vec::new();
        if req.simulate {
            let mut sim_owned = owned_items.clone();
            let mut sim_gold = gold;
            for _ in 0..capacity.saturating_sub(slot) {
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
                    mode,
                    capacity,
                    stable,
                    true,
                    false,
                )?;
                let best = if mode == GameMode::Arena {
                    if sim_owned.is_empty() {
                        sims.iter()
                            .find(|candidate| is_arena_starter(candidate.item))
                            .or_else(|| sims.iter().find(|candidate| sim_gold >= candidate.price))
                            .or_else(|| sims.first())
                    } else {
                        sims.iter()
                            .find(|candidate| sim_gold >= candidate.price)
                            .or_else(|| sims.first())
                    }
                } else {
                    sims.first()
                };
                let Some(best) = best else { break };
                let item = best.item;
                let mut reasons = vec![format!("Score {:.0}", best.score)];
                if best.prior_n >= 20 {
                    reasons.push(format!("Winrate {}", pct(best.winrate, 1)));
                }
                if best.replacement {
                    reasons.push("Mejora gratuita de la role quest".to_string());
                }
                let plan_slot = if best.replacement {
                    sim_owned.len()
                } else {
                    sim_owned.len() + 1
                };
                build_plan.push(json!({
                    "id": item.id,
                    "name": item.name,
                    "icon": item.icon,
                    "gold": best.price,
                    "slot": plan_slot,
                    "replacement": best.replacement,
                    "score": round(best.score, 1),
                    "reasons": reasons,
                }));
                if let Some(replaces) = best.replaces {
                    if let Some(existing) = sim_owned.iter_mut().find(|owned| owned.id == replaces)
                    {
                        *existing = item;
                    } else {
                        sim_owned.push(item);
                    }
                } else {
                    sim_owned.push(item);
                }
                sim_gold = (sim_gold - best.price).max(0);
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
        if let Some(note) = &role_note {
            notes.push(note.clone());
        }
        if mode == GameMode::Mayhem && self.mode_data_key(mode) != "mayhem" {
            notes.push(
                "Mayhem: sin partidas propias en la muestra; usa datos de ARAM y reglas para sus objetos exclusivos."
                    .to_string(),
            );
        }
        if candidates.is_empty() {
            notes.push(if full_upgrades {
                "No hay mejoras directas con los componentes actuales; vende uno para seguir la build."
                    .to_string()
            } else {
                "No hay objetos candidatos con los filtros actuales.".to_string()
            });
        }
        if full_upgrades {
            notes.push(
                "Inventario lleno: completa un legendario reutilizando los componentes que ya tienes."
                    .to_string(),
            );
        }
        if profile.get("known").and_then(Value::as_i64).unwrap_or(0) < 5 {
            notes.push("Faltan campeones enemigos: el perfil de daño es aproximado.".to_string());
        }

        let owned_ids: HashSet<i64> = owned_items.iter().map(|item| item.id).collect();
        let next_component = if analysis.is_some() && !full_upgrades && next_purchase.is_none() {
            self.next_component(&focus, gold, &owned_ids, mode)
        } else {
            Value::Null
        };
        let urgency = if full_upgrades {
            "upgrade"
        } else if next_purchase.is_some() {
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
            "slot": (slot + 1).min(capacity),
            "slotCap": capacity,
            "fullInventory": full_inventory,
            "gold": gold,
            "minute": minute,
            "enemyProfile": profile,
            "items": items_out,
            "nextPurchase": next_purchase.unwrap_or(Value::Null),
            "nextLegendary": next_legendary.unwrap_or(Value::Null),
            "legendaryOptions": legendary_options,
            "targetStatus": target_status,
            "bootsRecommendation": boots_recommendation,
            "sellStarter": sell_starter,
            "situationalItems": situational_items,
            "buildPlan": build_plan,
            "powerSpike": {
                "slot": (slot + 1).min(capacity),
                "n": spike.map(|value| value.n).unwrap_or(0),
                "winrate": spike.map(|value| value.winrate),
                "nearby": nearby,
            },
            "notes": notes,
        });
        if stable {
            output["recommendationMode"] = json!("model_first");
            output["recommendationSource"] =
                json!(if antiheal_source { "antiheal" } else { "model" });
            let needs_antiheal = analysis.as_ref().is_some_and(|analysis| {
                !analysis.healers.is_empty()
                    && !owned_items.iter().any(|item| is_grievous_item(item))
            });
            if needs_antiheal {
                if let Some(candidate) = candidates
                    .iter()
                    .filter(|candidate| is_grievous_for_champion(candidate.item, champ))
                    .find(|candidate| gold >= candidate.price)
                    .or_else(|| {
                        candidates
                            .iter()
                            .find(|candidate| is_grievous_for_champion(candidate.item, champ))
                    })
                {
                    output["antihealSuggestion"] =
                        self.format_item(candidate, gold, slot, &role, mode);
                }
            }
        }
        if !mode.is_classic() {
            output["gameMode"] = json!(mode.key());
            output["modeLabel"] = json!(mode.label());
            output["modeNotes"] = json!(mode.notes());
        }
        if mode == GameMode::Arena {
            output["arenaShop"] = json!({
                "components": false,
                "starterPrice": 500,
                "bootsPrice": 500,
                "legendaryPrice": 2500,
                "statAnvil": 750,
                "legendaryAnvil": 2000,
                "prismaticAnvil": 4000,
            });
        }
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

    pub fn dataset_age_days(&self) -> Option<i64> {
        let end = self
            .summary
            .get("dateRange")
            .and_then(Value::as_array)
            .and_then(|range| range.get(1))
            .and_then(Value::as_str)?;
        let date = chrono::NaiveDate::parse_from_str(end, "%Y-%m-%d").ok()?;
        Some((chrono::Utc::now().date_naive() - date).num_days())
    }

    pub fn data_status(&self) -> Value {
        let age = self.dataset_age_days();
        json!({
            "patch": self.patch,
            "dateRange": self.summary.get("dateRange").cloned().unwrap_or(json!([])),
            "games": self.summary.get("games").cloned().unwrap_or(json!(0)),
            "ageDays": age,
            "fresh": age.map(|days| days <= 45).unwrap_or(false),
            "maxAgeDays": 45,
            "models": {
                "draft": self.draft_model.is_some(),
                "build": self.build_model.is_some(),
                "notes": self.model_notes,
            },
        })
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

/// Parte de las partidas de cada clase (conjunto de etiquetas) que se juega en
/// cada rol. Clave: `class_role_key`.
fn build_tag_role_shares(
    champions: &[Champion],
    priors: &crate::data::Priors,
) -> HashMap<String, f64> {
    let classes: HashMap<&str, String> = champions
        .iter()
        .map(|champ| (champ.name.as_str(), class_key(&champ.tags)))
        .collect();
    let mut by_role: HashMap<(String, String), f64> = HashMap::new();
    let mut by_class: HashMap<String, f64> = HashMap::new();
    for (name, champ) in &priors.champions {
        let Some(class) = classes.get(name.as_str()) else {
            continue;
        };
        if class.is_empty() {
            continue;
        }
        for (role, stat) in &champ.roles {
            let games = stat.n.max(0) as f64;
            *by_role.entry((class.clone(), role.clone())).or_insert(0.0) += games;
            *by_class.entry(class.clone()).or_insert(0.0) += games;
        }
    }
    by_role
        .into_iter()
        .map(|((class, role), games)| {
            let total = by_class.get(&class).copied().unwrap_or(0.0);
            let share = if total > 0.0 { games / total } else { 0.0 };
            (class_role_key(&class, &role), share)
        })
        .collect()
}

/// Agrega las participaciones por rol (Grieta) y por modo (ARAM, Mayhem y Arena)
/// en grupos de clase y en un grupo global. Sirven para estimar objetos de
/// campeones con pocas partidas sin inventar datos.
fn build_share_groups(
    champions: &[Champion],
    priors: &crate::data::Priors,
    modes: &ModePriors,
) -> HashMap<String, ShareGroup> {
    let profiles: HashMap<&str, (&str, &str)> = champions
        .iter()
        .map(|champ| {
            let tag = champ.tags.first().map(String::as_str).unwrap_or("");
            let profile = damage_profile(&champ.damage, champ.info.attack, champ.info.magic);
            (champ.name.as_str(), (tag, profile))
        })
        .collect();
    let mut groups: HashMap<String, ShareGroup> = HashMap::new();
    for (name, champ) in &priors.champions {
        let (tag, profile) = profiles.get(name.as_str()).copied().unwrap_or(("", "HYB"));
        for (role, stat) in &champ.roles {
            if stat.n <= 0 {
                continue;
            }
            let role = role.as_str();
            let mut names = vec![
                group_name(&["classic", role, "*"]),
                group_name(&["classic", role, profile]),
            ];
            if !tag.is_empty() && stat.n >= crate::shares::MIN_GROUP_MEMBER_GAMES {
                names.push(group_name(&["classic", role, tag]));
                names.push(group_name(&["classic", role, tag, profile]));
            }
            let items = champ.items.get(role);
            for group_key in names {
                let group = groups.entry(group_key).or_default();
                group.add_games(stat.n as f64);
                if let Some(items) = items {
                    for (item_id, item) in items {
                        group.add_item(item_id, item.n as f64, item.avg_slot);
                    }
                }
            }
        }
    }
    for (mode, data) in &modes.modes {
        for (name, champ) in &data.champions {
            if champ.n <= 0 {
                continue;
            }
            let (tag, profile) = profiles.get(name.as_str()).copied().unwrap_or(("", "HYB"));
            let mode = mode.as_str();
            let mut names = vec![group_name(&[mode, "*"]), group_name(&[mode, profile])];
            if !tag.is_empty() {
                names.push(group_name(&[mode, tag]));
                names.push(group_name(&[mode, tag, profile]));
            }
            for group_key in names {
                let group = groups.entry(group_key).or_default();
                group.add_games(champ.n as f64);
                for (item_key, item) in &champ.items {
                    group.add_item(item_key, item.n as f64, item.avg_slot);
                }
            }
        }
    }
    groups
}

const MAYHEM_ITEM_IDS: [i64; 3] = [3039, 123430, 124011];
/// Participación supuesta de los objetos exclusivos de Mayhem sin partidas.
const MAYHEM_RULE_SHARE: f64 = 0.06;
/// Iniciales de Arena. Arena usa IDs propios para estos objetos (p. ej.
/// Guardian's Orb 223112), así que se reconocen por nombre.
const ARENA_STARTER_NAMES: [&str; 7] = [
    "guardianshorn",
    "guardianshammer",
    "guardiansblade",
    "guardiansorb",
    "guardiansshroud",
    "guardiansamulet",
    "guardiansdirk",
];
const SUPPORT_ITEM_IDS: [i64; 8] = [3865, 3866, 3867, 3869, 3870, 3871, 3876, 3877];
const SUPPORT_FINAL_IDS: [i64; 5] = [3869, 3870, 3871, 3876, 3877];
/// Botas tier 2 -> tier 3 que la role quest de mid mejora gratis.
const TIER3_BOOTS: [(i64, i64); 7] = [
    (3006, 3172), // Berserker's Greaves -> Gunmetal Greaves
    (3008, 3168), // Gluttonous Greaves -> Immortal Path
    (3009, 3170), // Boots of Swiftness -> Swiftmarch
    (3020, 3175), // Sorcerer's Shoes -> Spellslinger's Shoes
    (3047, 3174), // Plated Steelcaps -> Armored Advance
    (3111, 3173), // Mercury's Treads -> Chainlaced Crushers
    (3158, 3171), // Ionian Boots of Lucidity -> Crimson Lucidity
];

fn is_mayhem_item(item_id: i64) -> bool {
    MAYHEM_ITEM_IDS.contains(&item_id)
}

fn is_arena_starter(item: &Item) -> bool {
    ARENA_STARTER_NAMES.contains(&compact_name(&item.name).as_str())
}

fn is_support_item(item_id: i64) -> bool {
    SUPPORT_ITEM_IDS.contains(&item_id)
}

fn is_support_final(item_id: i64) -> bool {
    SUPPORT_FINAL_IDS.contains(&item_id)
}

fn is_jungle_pet(item_id: i64) -> bool {
    matches!(item_id, 1101..=1103)
}

fn is_doran_item(item: &Item) -> bool {
    compact_name(&item.name).starts_with("doran")
}

/// Reglas de inventario que dependen del rol, no solo del mapa.
///
/// - En Grieta, el objeto de soporte solo lo puede llevar el support y su
///   cadena de mejora es secuencial.
/// - Las mascotas de jungla solo las lleva el jungla.
/// - Doran y World Atlas no pueden convivir: el support no recibe Doran.
fn role_item_legal(item: &Item, role: &str, mode: GameMode, owned: &[&Item]) -> bool {
    if mode != GameMode::Classic {
        return true;
    }
    if is_support_item(item.id) {
        if role != "SUPPORT" {
            return false;
        }
        let current = owned
            .iter()
            .find(|owned| is_support_item(owned.id))
            .map(|owned| owned.id);
        return match current {
            None => item.id == 3865,
            Some(3865) => item.id == 3866,
            Some(3866) => item.id == 3867,
            Some(3867) => is_support_final(item.id),
            Some(_) => false,
        };
    }
    if is_jungle_pet(item.id) {
        return role == "JUNGLE";
    }
    if is_doran_item(item) {
        return role != "SUPPORT";
    }
    true
}

/// Ranuras de build disponibles. El bot tiene seis ranuras normales y,
/// tras completar la role quest, una septima dedicada a las botas.
fn build_capacity(mode: GameMode, role: &str, minute: f64, owned_len: usize) -> usize {
    if mode == GameMode::Classic
        && role == "BOT"
        && (role_quest_done(mode, role, minute) || owned_len >= 7)
    {
        7
    } else {
        6
    }
}

fn tier3_boots_upgrade(item_id: i64) -> Option<i64> {
    TIER3_BOOTS
        .iter()
        .find(|(tier2, _)| *tier2 == item_id)
        .map(|(_, tier3)| *tier3)
}

fn role_quest_done(mode: GameMode, role: &str, minute: f64) -> bool {
    mode == GameMode::Classic && matches!(role, "MID" | "BOT") && minute >= 16.0
}

fn item_from_meta(item: &MetaItem) -> Item {
    Item {
        idx: 0,
        id: item.id,
        name: item.name.clone(),
        icon: item.icon.clone(),
        gold: item.gold,
        tags: item.tags.clone(),
        stats: item.stats.clone(),
        from: item.from.clone(),
        into: item.into.clone(),
        boots: item.boots,
    }
}

fn recipe_progress(
    target: &Item,
    owned: &[&Item],
    items: &HashMap<i64, MetaItem>,
) -> RecipeProgress {
    let mut counts: HashMap<i64, usize> = HashMap::new();
    for item in owned {
        *counts.entry(item.id).or_insert(0) += 1;
    }
    let mut progress = RecipeProgress::default();
    for child in &target.from {
        consume_recipe(*child, &mut counts, items, &mut progress);
    }
    progress
}

fn consume_recipe(
    id: i64,
    counts: &mut HashMap<i64, usize>,
    items: &HashMap<i64, MetaItem>,
    progress: &mut RecipeProgress,
) {
    if let Some(count) = counts.get_mut(&id) {
        if *count > 0 {
            *count -= 1;
            progress.owned.push(id);
            progress.owned_gold += items.get(&id).map(|item| item.gold).unwrap_or(0);
            return;
        }
    }
    if let Some(meta) = items.get(&id) {
        for child in &meta.from {
            consume_recipe(*child, counts, items, progress);
        }
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

#[cfg(test)]
mod tests {
    use super::*;

    fn item(id: i64, name: &str, boots: bool) -> Item {
        Item {
            idx: 0,
            id,
            name: name.to_string(),
            icon: String::new(),
            gold: 400,
            tags: Vec::new(),
            stats: HashMap::new(),
            from: Vec::new(),
            into: Vec::new(),
            boots,
        }
    }

    #[test]
    fn support_chain_only_advances_one_step() {
        let world = item(3865, "World Atlas", false);
        let compass = item(3866, "Runic Compass", false);
        let bounty = item(3867, "Bounty of Worlds", false);
        let dream = item(3870, "Dream Maker", false);
        let none: Vec<&Item> = Vec::new();
        assert!(role_item_legal(&world, "SUPPORT", GameMode::Classic, &none));
        assert!(!role_item_legal(
            &compass,
            "SUPPORT",
            GameMode::Classic,
            &none
        ));
        let owned_world = vec![&world];
        assert!(role_item_legal(
            &compass,
            "SUPPORT",
            GameMode::Classic,
            &owned_world
        ));
        assert!(!role_item_legal(
            &bounty,
            "SUPPORT",
            GameMode::Classic,
            &owned_world
        ));
        let owned_compass = vec![&compass];
        assert!(role_item_legal(
            &bounty,
            "SUPPORT",
            GameMode::Classic,
            &owned_compass
        ));
        let owned_bounty = vec![&bounty];
        assert!(role_item_legal(
            &dream,
            "SUPPORT",
            GameMode::Classic,
            &owned_bounty
        ));
        assert!(!role_item_legal(&world, "MID", GameMode::Classic, &none));
    }

    fn meta_item(id: i64, name: &str, gold: i64, from: Vec<i64>) -> MetaItem {
        MetaItem {
            id,
            name: name.to_string(),
            gold,
            from,
            ..Default::default()
        }
    }

    #[test]
    fn recipe_counts_duplicate_components_and_nested_parts() {
        let mut items = HashMap::new();
        items.insert(1058, meta_item(1058, "Needlessly Large Rod", 1200, vec![]));
        items.insert(1052, meta_item(1052, "Amplifying Tome", 400, vec![]));
        items.insert(1027, meta_item(1027, "Sapphire Crystal", 350, vec![]));
        items.insert(
            3802,
            meta_item(3802, "Lost Chapter", 1200, vec![1052, 1027]),
        );

        let rod_a = item(1058, "Needlessly Large Rod", false);
        let rod_b = item(1058, "Needlessly Large Rod", false);
        let amp = item(1052, "Amplifying Tome", false);
        let sapphire = item(1027, "Sapphire Crystal", false);

        let mut rabadon = item(3089, "Rabadon's Deathcap", false);
        rabadon.gold = 3500;
        rabadon.from = vec![1058, 1058];
        let progress = recipe_progress(&rabadon, &[&rod_a, &rod_b, &amp], &items);
        assert_eq!(progress.owned, vec![1058, 1058]);
        assert_eq!(progress.owned_gold, 2400);

        let mut ludens = item(6655, "Luden's Echo", false);
        ludens.gold = 2750;
        ludens.from = vec![3802];
        let progress = recipe_progress(&ludens, &[&amp, &sapphire], &items);
        assert_eq!(progress.owned, vec![1052, 1027]);
        assert_eq!(progress.owned_gold, 750);
    }

    #[test]
    fn bot_capacity_uses_role_quest_window() {
        assert_eq!(build_capacity(GameMode::Classic, "BOT", 15.0, 6), 6);
        assert_eq!(build_capacity(GameMode::Classic, "BOT", 16.0, 6), 7);
        assert_eq!(build_capacity(GameMode::Classic, "BOT", 10.0, 7), 7);
        assert_eq!(build_capacity(GameMode::Classic, "MID", 20.0, 6), 6);
        assert_eq!(build_capacity(GameMode::Arena, "BOT", 20.0, 6), 6);
    }

    #[test]
    fn mid_boots_upgrade_is_exact() {
        assert_eq!(tier3_boots_upgrade(3006), Some(3172));
        assert_eq!(tier3_boots_upgrade(3008), Some(3168));
        assert_eq!(tier3_boots_upgrade(3158), Some(3171));
        assert_eq!(tier3_boots_upgrade(1001), None);
        assert!(role_quest_done(GameMode::Classic, "MID", 16.0));
        assert!(!role_quest_done(GameMode::Classic, "MID", 15.9));
        assert!(!role_quest_done(GameMode::Arena, "MID", 20.0));
    }

    #[test]
    fn classifies_enemy_build_damage() {
        assert_eq!(
            enemy_damage_label(80.0, 0.0, 0.25, 0.25, 0.0, 0.0, 0.0, false, false),
            "AD"
        );
        assert_eq!(
            enemy_damage_label(0.0, 150.0, 0.0, 0.0, 0.0, 0.0, 0.0, false, false),
            "AP"
        );
        assert_eq!(
            enemy_damage_label(40.0, 45.0, 0.0, 0.0, 0.0, 0.0, 0.0, false, false),
            "Mixto"
        );
        assert_eq!(
            enemy_damage_label(0.0, 0.0, 0.0, 0.0, 120.0, 80.0, 1200.0, true, false),
            "Tanque"
        );
        assert_eq!(
            enemy_damage_label(0.0, 20.0, 0.0, 0.0, 0.0, 0.0, 60.0, false, true),
            "Soporte"
        );
    }

    #[test]
    fn classifies_enemy_build_threat() {
        assert_eq!(enemy_threat_label(4, 2, 1, 3000), "Activo");
        assert_eq!(enemy_threat_label(6, 2, 2, 4000), "Fuerte");
        assert_eq!(enemy_threat_label(2, 1, 12, 3500), "Fuerte");
        assert_eq!(enemy_threat_label(0, 0, 3, 1500), "Activo");
        assert_eq!(enemy_threat_label(0, 2, 0, 900), "Normal");
    }

    fn champion_for_damage(damage: &str, tags: &[&str], attack: i64, magic: i64) -> Champion {
        Champion {
            idx: 0,
            id: "Test".to_string(),
            key: 0,
            name: "Test".to_string(),
            icon: String::new(),
            tags: tags.iter().map(|tag| tag.to_string()).collect(),
            damage: damage.to_string(),
            info: crate::data::ChampionInfo {
                attack,
                magic,
                ..Default::default()
            },
        }
    }

    #[test]
    fn rejects_off_damage_items() {
        let ad = champion_for_damage("AD", &["Fighter"], 7, 4);
        let ap = champion_for_damage("AP", &["Mage"], 4, 7);
        let mut rylai = item(3116, "Rylai's Crystal Scepter", false);
        rylai.tags = vec!["Health".to_string(), "SpellDamage".to_string()];
        rylai.stats.insert("FlatMagicDamageMod".to_string(), 70.0);
        let mut infinity = item(3031, "Infinity Edge", false);
        infinity.tags = vec!["Damage".to_string(), "CriticalStrike".to_string()];
        infinity
            .stats
            .insert("FlatPhysicalDamageMod".to_string(), 70.0);
        let mut spellslinger = item(3175, "Spellslinger's Shoes", true);
        spellslinger.tags = vec!["Boots".to_string(), "MagicPenetration".to_string()];

        assert!(!item_matches_champion_damage(&rylai, &ad));
        assert!(!item_matches_champion_damage(&spellslinger, &ad));
        assert!(item_matches_champion_damage(&infinity, &ad));
        assert!(item_matches_champion_damage(&rylai, &ap));
        assert!(!item_matches_champion_damage(&infinity, &ap));
    }

    #[test]
    fn arena_starters_are_recognized_by_name() {
        assert!(is_arena_starter(&item(223112, "Guardian's Orb", false)));
        assert!(is_arena_starter(&item(2051, "Guardian's Horn", false)));
        assert!(!is_arena_starter(&item(3026, "Guardian Angel", false)));
        assert!(!is_arena_starter(&item(3157, "Zhonya's Hourglass", false)));
    }

    #[test]
    fn healing_priority_distinguishes_healers_from_minor_sustain() {
        assert_eq!(healing_champion_points("Soraka", "Normal"), Some(2));
        assert_eq!(healing_champion_points("Sylas", "Normal"), Some(1));
        assert_eq!(healing_champion_points("Sylas", "Fuerte"), Some(2));
        assert_eq!(healing_champion_points("Irelia", "Fuerte"), None);
    }

    #[test]
    fn shield_priority_ignores_a_single_minor_shield() {
        assert!(!shield_reduction_priority(1, false, false));
        assert!(shield_reduction_priority(2, false, false));
        assert!(shield_reduction_priority(1, true, false));
        assert!(shield_reduction_priority(1, false, true));
        assert!(is_major_shield("Locket of the Iron Solari"));
        assert!(!is_major_shield("Eclipse"));
    }

    #[test]
    fn situational_categories_prioritize_build_answers() {
        assert_eq!(
            situational_category("Antiheal contra la curación de Vi"),
            "antiheal"
        );
        assert_eq!(
            situational_category("Penetración de armadura contra Gnar"),
            "armor_pen"
        );
        assert_eq!(
            situational_category("Penetración mágica contra la RM de Vi"),
            "magic_pen"
        );
        assert_eq!(
            situational_category("Reducción de escudos contra el equipo rival"),
            "shield"
        );
        assert_eq!(
            situational_category("Daño porcentual contra la vida de Gnar"),
            "hp"
        );
        assert!(
            situational_priority("Antiheal contra la curación rival")
                > situational_priority("Armadura contra el rival fuerte")
        );
    }
}
