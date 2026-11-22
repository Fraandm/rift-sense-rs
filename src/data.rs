use anyhow::{Context, Result, bail};
use indexmap::IndexMap;
use serde::Deserialize;
use serde_json::Value;
use std::collections::HashMap;
use std::path::Path;

use crate::config::Paths;

/// Normaliza nombres como el Python original: "Kai'Sa" -> "kaisa".
pub fn norm_name(value: &str) -> String {
    use unicode_normalization::UnicodeNormalization;
    let mut text = String::new();
    for ch in value.nfkd() {
        if !unicode_normalization::char::is_combining_mark(ch) {
            text.push(ch);
        }
    }
    text = text.replace(['\'', '\u{2019}', '`'], "").to_lowercase();
    text.chars()
        .filter(|ch| ch.is_ascii_alphanumeric())
        .collect()
}

#[derive(Deserialize, Clone, Debug, Default)]
pub struct ChampionInfo {
    #[serde(default)]
    pub attack: i64,
    #[serde(default)]
    pub defense: i64,
    #[serde(default)]
    pub magic: i64,
    #[serde(default)]
    pub difficulty: i64,
}

#[derive(Deserialize, Clone, Debug)]
pub struct Champion {
    pub idx: i64,
    pub id: String,
    pub key: i64,
    pub name: String,
    pub icon: String,
    #[serde(default)]
    pub tags: Vec<String>,
    #[serde(default)]
    pub damage: String,
    #[serde(default)]
    pub info: ChampionInfo,
}

#[derive(Deserialize, Clone, Debug, Default)]
pub struct Item {
    pub idx: i64,
    pub id: i64,
    pub name: String,
    pub icon: String,
    #[serde(default)]
    pub gold: i64,
    #[serde(default)]
    pub tags: Vec<String>,
    #[serde(default)]
    pub stats: HashMap<String, f64>,
    #[serde(default, rename = "from")]
    pub from: Vec<i64>,
    #[serde(default)]
    pub into: Vec<i64>,
    #[serde(default)]
    pub boots: bool,
}

#[derive(Deserialize, Clone, Debug, Default)]
pub struct Vocab {
    #[serde(default, rename = "ddragonVersion")]
    pub ddragon_version: String,
    #[serde(default, rename = "championList")]
    pub champion_list: Vec<Champion>,
    #[serde(default, rename = "itemList")]
    pub item_list: Vec<Item>,
    #[serde(default)]
    pub roles: Vec<String>,
    #[serde(default, rename = "roleIdx")]
    pub role_idx: HashMap<String, usize>,
}

#[derive(Deserialize, Clone, Debug, Default)]
pub struct Priors {
    #[serde(default)]
    pub games: i64,
    #[serde(default)]
    pub builds: i64,
    #[serde(default, rename = "dateRange")]
    pub date_range: Vec<String>,
    #[serde(default, rename = "roleBaseline")]
    pub role_baseline: IndexMap<String, RoleBaseline>,
    #[serde(default, rename = "itemCountWinrate")]
    pub item_count_winrate: IndexMap<String, ItemCount>,
    #[serde(default, rename = "firstItems")]
    pub first_items: Value,
    #[serde(default)]
    pub champions: IndexMap<String, ChampionPriors>,
    #[serde(default)]
    pub items: IndexMap<String, ItemPriors>,
}

fn half() -> f64 {
    0.5
}

#[derive(Deserialize, Clone, Debug, Default)]
pub struct RoleBaseline {
    #[serde(default)]
    pub n: i64,
    #[serde(default = "half")]
    pub winrate: f64,
}

#[derive(Deserialize, Clone, Debug, Default)]
pub struct RoleStat {
    #[serde(default)]
    pub n: i64,
    #[serde(default)]
    pub wins: i64,
    #[serde(default = "half")]
    pub winrate: f64,
}

#[derive(Deserialize, Clone, Debug, Default)]
pub struct MatchupStat {
    #[serde(default)]
    pub n: i64,
    #[serde(default)]
    pub wins: i64,
}

#[derive(Deserialize, Clone, Debug, Default)]
pub struct ChampionItemStat {
    #[serde(default)]
    pub n: i64,
    #[serde(default)]
    pub wins: i64,
    #[serde(default, rename = "avgSlot")]
    pub avg_slot: f64,
}

#[derive(Deserialize, Clone, Debug, Default)]
pub struct ChampionPriors {
    #[serde(default)]
    pub n: i64,
    #[serde(default)]
    pub wins: i64,
    #[serde(default = "half")]
    pub winrate: f64,
    #[serde(default)]
    pub bans: i64,
    #[serde(default)]
    pub roles: IndexMap<String, RoleStat>,
    #[serde(default)]
    pub matchups: IndexMap<String, MatchupStat>,
    #[serde(default)]
    pub synergy: IndexMap<String, MatchupStat>,
    /// role -> item_id -> estadisticas
    #[serde(default)]
    pub items: IndexMap<String, IndexMap<String, ChampionItemStat>>,
}

#[derive(Deserialize, Clone, Debug, Default)]
pub struct ItemRoleStat {
    #[serde(default)]
    pub n: i64,
    #[serde(default = "half")]
    pub winrate: f64,
    #[serde(default, rename = "avgSlot")]
    pub avg_slot: f64,
}

#[derive(Deserialize, Clone, Debug, Default)]
pub struct ItemPriors {
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub icon: String,
    #[serde(default)]
    pub gold: i64,
    #[serde(default)]
    pub n: i64,
    #[serde(default = "half")]
    pub winrate: f64,
    #[serde(default, rename = "avgSlot")]
    pub avg_slot: f64,
    #[serde(default)]
    pub roles: IndexMap<String, ItemRoleStat>,
}

#[derive(Deserialize, Clone, Debug, Default)]
pub struct ItemCount {
    #[serde(default)]
    pub n: i64,
    #[serde(default = "half")]
    pub winrate: f64,
}

#[derive(Deserialize, Clone, Debug, Default)]
pub struct MetaItem {
    pub id: i64,
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub icon: String,
    #[serde(default)]
    pub gold: i64,
    #[serde(default)]
    pub tags: Vec<String>,
    #[serde(default)]
    pub stats: HashMap<String, f64>,
    #[serde(default, rename = "from")]
    pub from: Vec<i64>,
    #[serde(default)]
    pub into: Vec<i64>,
    #[serde(default)]
    pub boots: bool,
    #[serde(default)]
    pub depth: i64,
    #[serde(default)]
    pub maps: HashMap<String, bool>,
    #[serde(default)]
    pub trainable: bool,
}

/// Todos los datos que necesita el motor, ya indexados.
pub struct Catalog {
    pub vocab: Vocab,
    pub priors: Priors,
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
}

impl Catalog {
    pub fn load(paths: &Paths) -> Result<Self> {
        let vocab: Vocab = crate::config::read_json(&paths.processed.join("vocab.json"))
            .context("Faltan datos procesados. Ejecuta el pipeline de datos.")?;
        let priors: Priors = crate::config::read_json(&paths.processed.join("priors.json"))
            .context("Faltan priors. Ejecuta el pipeline de datos.")?;
        if vocab.champion_list.is_empty() || vocab.item_list.is_empty() {
            bail!("El vocabulario esta vacio.");
        }

        let meta: Value = crate::config::read_json_or_default(&paths.artifacts.join("meta.json"))?;
        let summary: Value =
            crate::config::read_json_or_default(&paths.processed.join("summary.json"))?;
        let draft_report: Value =
            crate::config::read_json_or_default(&paths.artifacts.join("draft_report.json"))?;
        let build_report: Value =
            crate::config::read_json_or_default(&paths.artifacts.join("build_report.json"))?;
        let timings: Value =
            crate::config::read_json_or_default(&paths.processed.join("timings.json"))?;

        let role_labels = meta
            .get("roleLabels")
            .and_then(Value::as_object)
            .map(|map| {
                map.iter()
                    .filter_map(|(k, v)| v.as_str().map(|s| (k.clone(), s.to_string())))
                    .collect()
            })
            .unwrap_or_default();
        let icon_base = meta
            .get("iconBase")
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_string();
        let patch = meta
            .get("ddragonVersion")
            .and_then(Value::as_str)
            .map(str::to_string)
            .filter(|s| !s.is_empty())
            .unwrap_or_else(|| vocab.ddragon_version.clone());

        let mut champ_by_norm = HashMap::with_capacity(vocab.champion_list.len());
        let mut champ_by_key = HashMap::with_capacity(vocab.champion_list.len());
        for (index, champ) in vocab.champion_list.iter().enumerate() {
            champ_by_norm.insert(norm_name(&champ.name), index);
            champ_by_key.insert(champ.key, index);
        }
        let mut item_by_id = HashMap::with_capacity(vocab.item_list.len());
        let mut item_by_norm = HashMap::with_capacity(vocab.item_list.len());
        for (index, item) in vocab.item_list.iter().enumerate() {
            item_by_id.insert(item.id, index);
            item_by_norm.insert(norm_name(&item.name), index);
        }
        let all_items_by_id = meta
            .get("items")
            .and_then(Value::as_array)
            .map(|items| {
                items
                    .iter()
                    .filter_map(|value| serde_json::from_value::<MetaItem>(value.clone()).ok())
                    .map(|item| (item.id, item))
                    .collect()
            })
            .unwrap_or_default();

        let priors_games = priors.games.max(1) as f64;
        let unk = vocab.champion_list.len() as i64 + 1;
        let champions = vocab.champion_list.clone();
        let items = vocab.item_list.clone();
        let roles = vocab.roles.clone();
        let role_idx = vocab.role_idx.clone();

        Ok(Self {
            vocab,
            priors,
            meta,
            summary,
            draft_report,
            build_report,
            timings,
            champions,
            items,
            roles,
            role_labels,
            role_idx,
            unk,
            champ_by_norm,
            champ_by_key,
            item_by_id,
            item_by_norm,
            all_items_by_id,
            priors_games,
            patch,
            icon_base,
        })
    }

    pub fn champion(&self, index: usize) -> &Champion {
        &self.champions[index]
    }

    pub fn item(&self, index: usize) -> &Item {
        &self.items[index]
    }

    pub fn find_champion(&self, value: &crate::engine::ChampionRef) -> Option<&Champion> {
        use crate::engine::ChampionRef;
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

    pub fn find_item(&self, value: &crate::engine::ItemRef) -> Option<&Item> {
        use crate::engine::ItemRef;
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

    pub fn meta_item(&self, id: i64) -> Option<&MetaItem> {
        self.all_items_by_id.get(&id)
    }

    pub fn item_gold(&self, id: i64) -> i64 {
        self.all_items_by_id
            .get(&id)
            .map(|item| item.gold)
            .or_else(|| self.item_by_id.get(&id).map(|i| self.items[*i].gold))
            .unwrap_or(0)
    }

    pub fn item_name(&self, id: i64) -> String {
        self.all_items_by_id
            .get(&id)
            .map(|item| item.name.clone())
            .or_else(|| {
                self.item_by_id
                    .get(&id)
                    .map(|i| self.items[*i].name.clone())
            })
            .unwrap_or_default()
    }

    pub fn item_icon(&self, id: i64) -> String {
        self.all_items_by_id
            .get(&id)
            .map(|item| item.icon.clone())
            .or_else(|| {
                self.item_by_id
                    .get(&id)
                    .map(|i| self.items[*i].icon.clone())
            })
            .unwrap_or_default()
    }

    pub fn draft_model_path(&self, paths: &Paths) -> std::path::PathBuf {
        paths.draft_model()
    }
}

pub fn read_manifest(models_dir: &Path) -> Option<serde_json::Value> {
    let path = models_dir.join("manifest.json");
    if !path.is_file() {
        return None;
    }
    crate::config::read_json(&path).ok()
}
