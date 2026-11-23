//! Catálogo de runas y recomendación por campeón, rol y composición rival.

use anyhow::{Context, Result};
use serde::Deserialize;
use serde_json::{Value, json};
use std::collections::{HashMap, HashSet};

use crate::data::Champion;

#[derive(Clone, Debug, Deserialize)]
struct RuneTree {
    id: i64,
    key: String,
    name: String,
    icon: String,
}

#[derive(Clone, Debug, Deserialize)]
struct RuneDef {
    id: i64,
    key: String,
    name: String,
    icon: String,
    tree: i64,
    slot: i64,
}

#[derive(Clone, Debug, Deserialize)]
struct StatShard {
    id: i64,
    name: String,
    icon: String,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct HistoricalPage {
    keystone: i64,
    primary_tree: i64,
    secondary_tree: i64,
    #[serde(default)]
    primary_runes: Vec<i64>,
    #[serde(default)]
    secondary_runes: Vec<i64>,
    #[serde(default)]
    shards: Vec<i64>,
    #[serde(default)]
    n: i64,
    #[serde(default)]
    wins: i64,
}

#[derive(Clone, Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RuneCatalog {
    #[serde(default)]
    patch: String,
    #[serde(default)]
    trees: Vec<RuneTree>,
    #[serde(default)]
    runes: Vec<RuneDef>,
    #[serde(default)]
    stat_shards: Vec<StatShard>,
    #[serde(default)]
    champion_pages: HashMap<String, HashMap<String, HistoricalPage>>,
    #[serde(skip)]
    tree_by_id: HashMap<i64, usize>,
    #[serde(skip)]
    tree_by_key: HashMap<String, usize>,
    #[serde(skip)]
    rune_by_id: HashMap<i64, usize>,
    #[serde(skip)]
    rune_by_key: HashMap<String, usize>,
    #[serde(skip)]
    shard_by_id: HashMap<i64, usize>,
}

#[derive(Clone, Debug)]
struct PageData {
    keystone: i64,
    primary_tree: i64,
    secondary_tree: i64,
    primary_runes: Vec<i64>,
    secondary_runes: Vec<i64>,
    shards: Vec<i64>,
    n: i64,
    wins: i64,
    source: &'static str,
}

impl RuneCatalog {
    pub fn load() -> Result<Self> {
        let mut catalog: Self = serde_json::from_str(include_str!("../web/assets/data/runes.json"))
            .context("No se pudo leer el catálogo de runas embebido")?;
        catalog.index();
        Ok(catalog)
    }

    fn index(&mut self) {
        self.tree_by_id = self
            .trees
            .iter()
            .enumerate()
            .map(|(index, tree)| (tree.id, index))
            .collect();
        self.tree_by_key = self
            .trees
            .iter()
            .enumerate()
            .map(|(index, tree)| (tree.key.clone(), index))
            .collect();
        self.rune_by_id = self
            .runes
            .iter()
            .enumerate()
            .map(|(index, rune)| (rune.id, index))
            .collect();
        self.rune_by_key = self
            .runes
            .iter()
            .enumerate()
            .map(|(index, rune)| (rune.key.clone(), index))
            .collect();
        self.shard_by_id = self
            .stat_shards
            .iter()
            .enumerate()
            .map(|(index, shard)| (shard.id, index))
            .collect();
    }

    pub fn patch(&self) -> &str {
        &self.patch
    }

    #[allow(clippy::too_many_arguments)]
    pub fn recommend(
        &self,
        champion: &Champion,
        role: &str,
        physical: f64,
        magic: f64,
        enemy_tanks: usize,
        enemy_known: usize,
    ) -> Option<Value> {
        let mut page = self
            .historical(champion, role)
            .filter(|page| self.valid_page(page))
            .or_else(|| self.fallback(champion, role))?;
        let mut notes = Vec::new();
        self.adjust(
            &mut page,
            physical,
            magic,
            enemy_tanks,
            enemy_known,
            &mut notes,
        );
        self.to_json(champion, role, page, notes)
    }

    fn historical(&self, champion: &Champion, role: &str) -> Option<PageData> {
        let page = self.champion_pages.get(&champion.id)?.get(role)?;
        if page.n < 2 || !self.rune_by_id.contains_key(&page.keystone) {
            return None;
        }
        Some(PageData {
            keystone: page.keystone,
            primary_tree: page.primary_tree,
            secondary_tree: page.secondary_tree,
            primary_runes: page.primary_runes.clone(),
            secondary_runes: page.secondary_runes.clone(),
            shards: page.shards.clone(),
            n: page.n,
            wins: page.wins,
            source: "history",
        })
    }

    fn valid_page(&self, page: &PageData) -> bool {
        let keystone = self.rune_by_id.get(&page.keystone);
        let Some(keystone) = keystone.map(|index| &self.runes[*index]) else {
            return false;
        };
        if keystone.tree != page.primary_tree {
            return false;
        }
        if page.primary_runes.len() < 3 || page.secondary_runes.len() < 2 || page.shards.len() < 3 {
            return false;
        }
        if page.primary_runes.iter().any(|id| {
            self.rune_by_id
                .get(id)
                .map(|index| self.runes[*index].tree != page.primary_tree)
                .unwrap_or(true)
        }) {
            return false;
        }
        let mut secondary_slots = HashSet::new();
        page.secondary_runes.iter().all(|id| {
            self.rune_by_id
                .get(id)
                .map(|index| {
                    let rune = &self.runes[*index];
                    rune.tree == page.secondary_tree && secondary_slots.insert(rune.slot)
                })
                .unwrap_or(false)
        })
    }

    fn fallback(&self, champion: &Champion, role: &str) -> Option<PageData> {
        let tags: HashSet<&str> = champion.tags.iter().map(String::as_str).collect();
        let tank = tags.contains("Tank");
        let support = tags.contains("Support");
        let marksman = tags.contains("Marksman");
        let assassin = tags.contains("Assassin");
        let fighter = tags.contains("Fighter");
        let mage = tags.contains("Mage");

        let (primary_tree, keystone, secondary_tree, primary, secondary, shards) = if support {
            if tank {
                (
                    "Resolve",
                    "Guardian",
                    "Inspiration",
                    &["FontOfLife", "BonePlating", "Revitalize"][..],
                    &["MagicalFootwear", "CosmicInsight"][..],
                    &[5007, 5008, 5001][..],
                )
            } else {
                (
                    "Sorcery",
                    "SummonAery",
                    "Resolve",
                    &["ManaflowBand", "Transcendence", "Scorch"][..],
                    &["BonePlating", "Revitalize"][..],
                    &[5007, 5008, 5001][..],
                )
            }
        } else {
            match role {
                "BOT" if marksman => (
                    "Precision",
                    "PressTheAttack",
                    "Sorcery",
                    &["Triumph", "LegendAlacrity", "CoupDeGrace"][..],
                    &["ManaflowBand", "GatheringStorm"][..],
                    &[5005, 5008, 5001][..],
                ),
                "BOT" if mage => (
                    "Sorcery",
                    "ArcaneComet",
                    "Inspiration",
                    &["ManaflowBand", "Transcendence", "Scorch"][..],
                    &["MagicalFootwear", "CosmicInsight"][..],
                    &[5008, 5008, 5001][..],
                ),
                "TOP" if tank => (
                    "Resolve",
                    "GraspOfTheUndying",
                    "Precision",
                    &["Demolish", "BonePlating", "Overgrowth"][..],
                    &["Triumph", "LastStand"][..],
                    &[5008, 5008, 5001][..],
                ),
                "TOP" if marksman => (
                    "Precision",
                    "PressTheAttack",
                    "Sorcery",
                    &["Triumph", "LegendAlacrity", "CoupDeGrace"][..],
                    &["ManaflowBand", "GatheringStorm"][..],
                    &[5005, 5008, 5001][..],
                ),
                "TOP" if mage => (
                    "Sorcery",
                    "ArcaneComet",
                    "Inspiration",
                    &["ManaflowBand", "Transcendence", "Scorch"][..],
                    &["MagicalFootwear", "CosmicInsight"][..],
                    &[5008, 5008, 5001][..],
                ),
                "JUNGLE" if tank => (
                    "Resolve",
                    "Aftershock",
                    "Precision",
                    &["FontOfLife", "BonePlating", "Overgrowth"][..],
                    &["Triumph", "LastStand"][..],
                    &[5008, 5008, 5001][..],
                ),
                "JUNGLE" if assassin => (
                    "Domination",
                    "Electrocute",
                    "Sorcery",
                    &["TasteOfBlood", "GrislyMementos", "TreasureHunter"][..],
                    &["ManaflowBand", "Scorch"][..],
                    &[5008, 5008, 5001][..],
                ),
                "JUNGLE" if mage => (
                    "Domination",
                    "Electrocute",
                    "Sorcery",
                    &["TasteOfBlood", "GrislyMementos", "TreasureHunter"][..],
                    &["ManaflowBand", "Scorch"][..],
                    &[5008, 5008, 5001][..],
                ),
                "MID" if assassin => (
                    "Domination",
                    "Electrocute",
                    "Sorcery",
                    &["TasteOfBlood", "GrislyMementos", "TreasureHunter"][..],
                    &["ManaflowBand", "Scorch"][..],
                    &[5008, 5008, 5001][..],
                ),
                "MID" if mage => (
                    "Sorcery",
                    "ArcaneComet",
                    "Inspiration",
                    &["ManaflowBand", "Transcendence", "Scorch"][..],
                    &["MagicalFootwear", "CosmicInsight"][..],
                    &[5008, 5008, 5001][..],
                ),
                "MID" if marksman => (
                    "Precision",
                    "PressTheAttack",
                    "Sorcery",
                    &["Triumph", "LegendAlacrity", "CoupDeGrace"][..],
                    &["ManaflowBand", "GatheringStorm"][..],
                    &[5005, 5008, 5001][..],
                ),
                _ if fighter || assassin => (
                    "Precision",
                    "Conqueror",
                    "Resolve",
                    &["Triumph", "LegendAlacrity", "LastStand"][..],
                    &["BonePlating", "Overgrowth"][..],
                    &[5008, 5008, 5001][..],
                ),
                _ => (
                    "Precision",
                    "Conqueror",
                    "Resolve",
                    &["Triumph", "LegendAlacrity", "LastStand"][..],
                    &["BonePlating", "Overgrowth"][..],
                    &[5008, 5008, 5001][..],
                ),
            }
        };

        let primary_tree_id = *self.tree_by_key.get(primary_tree)?;
        let secondary_tree_id = *self.tree_by_key.get(secondary_tree)?;
        let keystone_id = self.runes[*self.rune_by_key.get(keystone)?].id;
        let primary_runes: Vec<i64> = primary
            .iter()
            .filter_map(|key| {
                self.rune_by_key
                    .get(*key)
                    .map(|index| self.runes[*index].id)
            })
            .collect();
        let secondary_runes: Vec<i64> = secondary
            .iter()
            .filter_map(|key| {
                self.rune_by_key
                    .get(*key)
                    .map(|index| self.runes[*index].id)
            })
            .collect();
        if primary_runes.len() < 3 || secondary_runes.len() < 2 {
            return None;
        }
        Some(PageData {
            keystone: keystone_id,
            primary_tree: self.trees[primary_tree_id].id,
            secondary_tree: self.trees[secondary_tree_id].id,
            primary_runes,
            secondary_runes,
            shards: shards.to_vec(),
            n: 0,
            wins: 0,
            source: "rules",
        })
    }

    fn adjust(
        &self,
        page: &mut PageData,
        physical: f64,
        magic: f64,
        enemy_tanks: usize,
        enemy_known: usize,
        notes: &mut Vec<String>,
    ) {
        if enemy_known > 0 && page.shards.len() >= 3 {
            let defense = if physical >= 0.60 {
                5002
            } else if magic >= 0.60 {
                5003
            } else {
                5001
            };
            if page.shards[2] != defense {
                page.shards[2] = defense;
                notes.push(match defense {
                    5002 => "Fragmento de armadura por el daño físico rival".to_string(),
                    5003 => "Fragmento de resistencia mágica por el daño mágico rival".to_string(),
                    _ => "Fragmento de vida por una composición mixta".to_string(),
                });
            }
        }

        if enemy_tanks >= 2 {
            let cut_down = self
                .rune_by_key
                .get("CutDown")
                .map(|index| self.runes[*index].id);
            let mut swapped = false;
            let already = cut_down.is_some_and(|cut_down| {
                page.primary_runes.contains(&cut_down) || page.secondary_runes.contains(&cut_down)
            });
            if page.primary_tree == 8000 {
                if let Some(cut_down) = cut_down {
                    if page.primary_runes.len() >= 3 && page.primary_runes[2] != cut_down {
                        page.primary_runes[2] = cut_down;
                        swapped = true;
                    }
                }
            } else if page.secondary_tree == 8000 {
                if let Some(cut_down) = cut_down {
                    if let Some(index) = page.secondary_runes.iter().position(|rune| {
                        self.rune_by_id
                            .get(rune)
                            .is_some_and(|index| self.runes[*index].slot == 3)
                    }) {
                        page.secondary_runes[index] = cut_down;
                        swapped = true;
                    }
                }
            }
            if swapped || already {
                notes.push("Derribado contra 2+ tanques rivales".to_string());
            } else {
                notes.push("Contra 2+ tanques, considera Derribado en Precisión".to_string());
            }
        }
    }

    fn to_json(
        &self,
        champion: &Champion,
        role: &str,
        page: PageData,
        mut notes: Vec<String>,
    ) -> Option<Value> {
        let keystone = self.rune_value(page.keystone)?;
        let primary_tree = self.tree_value(page.primary_tree)?;
        let secondary_tree = self.tree_value(page.secondary_tree)?;
        let primary_runes: Vec<Value> = page
            .primary_runes
            .iter()
            .filter_map(|id| self.rune_value(*id))
            .collect();
        let secondary_runes: Vec<Value> = page
            .secondary_runes
            .iter()
            .filter_map(|id| self.rune_value(*id))
            .collect();
        let shards: Vec<Value> = page
            .shards
            .iter()
            .filter_map(|id| self.shard_value(*id))
            .collect();
        if primary_runes.len() < 3 || secondary_runes.len() < 2 || shards.len() < 3 {
            return None;
        }

        if page.source == "history" {
            let winrate = if page.n > 0 {
                page.wins as f64 / page.n as f64 * 100.0
            } else {
                0.0
            };
            notes.insert(
                0,
                if page.n >= 5 {
                    format!(
                        "Basado en {} partidas recientes ({:.0}% WR)",
                        page.n, winrate
                    )
                } else {
                    format!("Muestra baja: {} partidas; página orientativa", page.n)
                },
            );
        } else {
            notes.insert(
                0,
                "Sin muestra histórica suficiente: página base por campeón y rol".to_string(),
            );
        }

        Some(json!({
            "champion": {
                "id": champion.id,
                "name": champion.name,
                "icon": champion.icon,
            },
            "role": role,
            "source": page.source,
            "sample": page.n,
            "wins": page.wins,
            "patch": self.patch,
            "primaryTree": primary_tree,
            "secondaryTree": secondary_tree,
            "keystone": keystone,
            "primaryRunes": primary_runes,
            "secondaryRunes": secondary_runes,
            "shards": shards,
            "notes": notes,
        }))
    }

    fn tree_value(&self, id: i64) -> Option<Value> {
        let tree = self.trees.get(*self.tree_by_id.get(&id)?)?;
        Some(json!({
            "id": tree.id,
            "key": tree.key,
            "name": tree.name,
            "icon": tree.icon,
        }))
    }

    fn rune_value(&self, id: i64) -> Option<Value> {
        let rune = self.runes.get(*self.rune_by_id.get(&id)?)?;
        Some(json!({
            "id": rune.id,
            "key": rune.key,
            "name": rune.name,
            "icon": rune.icon,
            "tree": rune.tree,
            "slot": rune.slot,
        }))
    }

    fn shard_value(&self, id: i64) -> Option<Value> {
        let shard = self.stat_shards.get(*self.shard_by_id.get(&id)?)?;
        Some(json!({
            "id": shard.id,
            "name": shard.name,
            "icon": shard.icon,
        }))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::data::ChampionInfo;

    fn champion(id: &str, name: &str, tags: &[&str], damage: &str) -> Champion {
        Champion {
            idx: 0,
            id: id.to_string(),
            key: 0,
            name: name.to_string(),
            icon: format!("{name}.png"),
            tags: tags.iter().map(|tag| tag.to_string()).collect(),
            damage: damage.to_string(),
            info: ChampionInfo::default(),
        }
    }

    #[test]
    fn loads_embedded_catalog() {
        let catalog = RuneCatalog::load().unwrap();
        assert_eq!(catalog.patch, "16.23.1");
        assert!(catalog.trees.len() >= 5);
        assert!(catalog.runes.len() > 40);
        assert!(catalog.champion_pages.contains_key("Ahri"));
    }

    #[test]
    fn uses_history_for_ahri_mid() {
        let catalog = RuneCatalog::load().unwrap();
        let ahri = champion("Ahri", "Ahri", &["Mage", "Assassin"], "AP");
        let page = catalog
            .recommend(&ahri, "MID", 0.5, 0.5, 0, 5)
            .expect("página");
        assert_eq!(page["source"], "history");
        assert_eq!(page["keystone"]["key"], "Electrocute");
        assert_eq!(page["primaryTree"]["key"], "Domination");
    }

    #[test]
    fn falls_back_for_unknown_champion() {
        let catalog = RuneCatalog::load().unwrap();
        let champ = champion("TestMage", "Test Mage", &["Mage"], "AP");
        let page = catalog
            .recommend(&champ, "MID", 0.5, 0.5, 0, 5)
            .expect("página");
        assert_eq!(page["source"], "rules");
        assert_eq!(page["keystone"]["key"], "ArcaneComet");
    }

    #[test]
    fn adapts_shards_and_cut_down() {
        let catalog = RuneCatalog::load().unwrap();
        let jinx = champion("Jinx", "Jinx", &["Marksman"], "AD");
        let page = catalog
            .recommend(&jinx, "BOT", 0.72, 0.28, 2, 5)
            .expect("página");
        assert_eq!(page["shards"][2]["id"], 5002);
        assert!(
            page["primaryRunes"]
                .as_array()
                .unwrap()
                .iter()
                .any(|rune| rune["key"] == "CutDown")
        );
        assert!(
            page["notes"]
                .as_array()
                .unwrap()
                .iter()
                .any(|note| note.as_str().unwrap_or("").contains("armadura"))
        );
    }
}
