//! Participación de cada objeto en las builds comparables.
//!
//! La participación de un objeto es la fracción de partidas de un campeón
//! (en un rol o en un modo) en las que aparece. Si el campeón tiene pocas
//! partidas, esa cifra se suaviza hacia la media de campeones parecidos
//! (misma clase principal en el mismo rol o modo). Así una combinación rara
//! no inventa objetos a partir de ruido, y el ranking no depende de
//! normalizar cada lista contra sí misma.

use std::collections::HashMap;

use serde::Deserialize;

/// Partidas "virtuales" que aportan la media del grupo.
pub const SHRINK_K: f64 = 30.0;
/// Participación mínima para mantener un objeto sin evidencia propia.
pub const MIN_SHARE: f64 = 0.012;
/// Partidas propias con un objeto que bastan para mantenerlo.
pub const MIN_OWN_GAMES: i64 = 5;
/// Partidas de un campeón en un rol para no cambiar de rol.
pub const MIN_ROLE_GAMES: i64 = 20;
/// Partidas de un campeón en un rol para fiarse del modelo de builds.
pub const MODEL_RELIABLE_GAMES: i64 = 30;
/// Partidas mínimas de un grupo para usarlo antes que la media global.
pub const MIN_GROUP_GAMES: f64 = 100.0;
/// Partidas de un campeón en un rol para que aporte a la media de su clase.
pub const MIN_GROUP_MEMBER_GAMES: i64 = 30;
/// Parte de las partidas de una clase que se juegan en un rol para considerarlo
/// habitual (p. ej. el 2 % de las partidas de Support+Tank se juega en BOT).
pub const MIN_CLASS_ROLE_SHARE: f64 = 0.05;
/// Partidas en otro rol que bastan para tomarlo como rol habitual del campeón.
pub const MIN_PRIMARY_GAMES: i64 = 10;

/// Clase de un campeón: el conjunto ordenado de sus etiquetas. Usar solo la
/// primera mezclaba, por ejemplo, a Senna (Support y Marksman, juega BOT) con
/// Thresh (Support y Tank, juega soporte).
pub fn class_key(tags: &[String]) -> String {
    let mut sorted: Vec<&str> = tags.iter().map(String::as_str).collect();
    sorted.sort_unstable();
    sorted.join("|")
}

/// Clave de la parte de una clase que juega un rol.
pub fn class_role_key(class: &str, role: &str) -> String {
    format!("{class}#{role}")
}
/// Apariciones de un objeto en partidas reales de un modo para considerarlo legal.
pub const MIN_OBSERVED: i64 = 3;

/// Objeto dentro de los datos de un modo (ARAM, Mayhem o Arena).
#[derive(Clone, Debug, Default, Deserialize)]
pub struct ModeItem {
    #[serde(default)]
    pub n: i64,
    #[serde(default)]
    pub wins: i64,
    #[serde(default, rename = "avgSlot")]
    pub avg_slot: f64,
    #[serde(default)]
    pub id: i64,
}

/// Campeón dentro de los datos de un modo.
#[derive(Clone, Debug, Default, Deserialize)]
pub struct ModeChampion {
    #[serde(default)]
    pub n: i64,
    #[serde(default)]
    pub wins: i64,
    #[serde(default)]
    pub items: HashMap<String, ModeItem>,
}

/// ID observado en partidas reales del modo.
#[derive(Clone, Debug, Default, Deserialize)]
pub struct ObservedId {
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub count: i64,
    #[serde(default)]
    pub maps: Vec<String>,
    #[serde(default)]
    pub purchasable: bool,
}

/// Datos de un modo generados por `lolanalyzer.data.mode_priors`.
#[derive(Clone, Debug, Default, Deserialize)]
pub struct ModeData {
    #[serde(default)]
    pub matches: i64,
    #[serde(default)]
    pub participants: i64,
    #[serde(default)]
    pub champions: HashMap<String, ModeChampion>,
    #[serde(default)]
    pub items: HashMap<String, ModeItem>,
    #[serde(default, rename = "observedIds")]
    pub observed_ids: HashMap<String, ObservedId>,
}

/// Contenido de `data/processed/priors_modes.json`.
#[derive(Clone, Debug, Default, Deserialize)]
pub struct ModePriors {
    #[serde(default)]
    pub modes: HashMap<String, ModeData>,
}

/// Agregado de participación para un grupo de campeones (clase y rol o modo).
#[derive(Clone, Debug, Default)]
pub struct ShareGroup {
    pub games: f64,
    pub counts: HashMap<String, f64>,
    pub slot_sums: HashMap<String, f64>,
}

impl ShareGroup {
    pub fn add_games(&mut self, games: f64) {
        self.games += games;
    }

    pub fn add_item(&mut self, key: &str, count: f64, avg_slot: f64) {
        if count <= 0.0 {
            return;
        }
        *self.counts.entry(key.to_string()).or_insert(0.0) += count;
        *self.slot_sums.entry(key.to_string()).or_insert(0.0) += count * avg_slot;
    }

    pub fn share(&self, key: &str) -> f64 {
        if self.games <= 0.0 {
            return 0.0;
        }
        self.counts.get(key).copied().unwrap_or(0.0) / self.games
    }

    pub fn avg_slot(&self, key: &str) -> Option<f64> {
        let count = *self.counts.get(key)?;
        if count <= 0.0 {
            return None;
        }
        Some(self.slot_sums.get(key)? / count)
    }
}

/// Participación suavizada: las partidas propias pesan más cuantas más haya.
pub fn shrink(own: f64, games: f64, fallback: f64) -> f64 {
    (own + SHRINK_K * fallback) / (games + SHRINK_K)
}

/// Convierte una participación en puntos de popularidad (0..1).
///
/// Usa raíz cuadrada para que un objeto que aparece en el 10 % de las builds
/// ya cuente de forma visible, y satura cuando llega a `scale`.
pub fn popularity_points(share: f64, scale: f64) -> f64 {
    if scale <= 0.0 {
        return 0.0;
    }
    (share.max(0.0) / scale).sqrt().min(1.0)
}

/// Perfil de daño del campeón: "AD", "AP" o "HYB".
///
/// Los campeones híbridos se clasifican por la diferencia entre ataque y
/// magia de Data Dragon, para no tratar a un mago como un tirador.
pub fn damage_profile(damage: &str, attack: i64, magic: i64) -> &'static str {
    match damage {
        "AD" => "AD",
        "AP" => "AP",
        _ if magic >= attack + 2 => "AP",
        _ if attack >= magic + 2 => "AD",
        _ => "HYB",
    }
}

/// Rol que usan los datos para un campeón.
///
/// Se mantiene el rol pedido si el campeón tiene partidas suficientes en él,
/// o si su clase (primera etiqueta: Support, Fighter, Marksman…) juega ese rol
/// con una frecuencia razonable. Si no, se usa su rol habitual: así no se
/// mezclan objetos de un ADC con un soporte raro. Devuelve el rol y si ha
/// cambiado.
pub fn data_role(
    requested: &str,
    requested_games: i64,
    class_role_share: f64,
    primary: &str,
    primary_games: i64,
) -> (String, bool) {
    if requested_games >= MIN_ROLE_GAMES || class_role_share >= MIN_CLASS_ROLE_SHARE {
        return (requested.to_string(), false);
    }
    if primary != requested && primary_games >= MIN_PRIMARY_GAMES {
        return (primary.to_string(), true);
    }
    (requested.to_string(), false)
}

/// Pesos de cada señal en la puntuación final (suman 1 en cada variante).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ScoreWeights {
    pub popularity: f64,
    pub slot: f64,
    pub winrate: f64,
    pub defense: f64,
    pub context: f64,
    pub model: f64,
    pub mode: f64,
}

/// `stable` prioriza el modelo y los datos; `classic` distingue Grieta de los
/// modos sin roles, donde el objeto depende más del modo.
pub fn score_weights(stable: bool, classic: bool) -> ScoreWeights {
    match (stable, classic) {
        (true, true) => ScoreWeights {
            popularity: 0.38,
            slot: 0.18,
            winrate: 0.08,
            defense: 0.10,
            context: 0.10,
            model: 0.16,
            mode: 0.0,
        },
        (false, true) => ScoreWeights {
            popularity: 0.48,
            slot: 0.22,
            winrate: 0.06,
            defense: 0.10,
            context: 0.14,
            model: 0.0,
            mode: 0.0,
        },
        (true, false) => ScoreWeights {
            popularity: 0.36,
            slot: 0.16,
            winrate: 0.08,
            defense: 0.06,
            context: 0.08,
            model: 0.0,
            mode: 0.26,
        },
        (false, false) => ScoreWeights {
            popularity: 0.44,
            slot: 0.20,
            winrate: 0.06,
            defense: 0.06,
            context: 0.08,
            model: 0.0,
            mode: 0.16,
        },
    }
}

/// Nombre de un grupo de campeones: sus partes separadas por `|`.
pub fn group_name(parts: &[&str]) -> String {
    parts.join("|")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shrink_without_games_uses_the_group_share() {
        assert!((shrink(0.0, 0.0, 0.4) - 0.4).abs() < 1e-9);
    }

    #[test]
    fn small_samples_move_toward_the_group() {
        let value = shrink(2.0, 3.0, 0.5);
        assert!(value > 0.45 && value < 0.55, "{value}");
    }

    #[test]
    fn large_samples_keep_the_champion_share() {
        let value = shrink(300.0, 1000.0, 0.1);
        assert!((value - 0.294).abs() < 0.01, "{value}");
    }

    #[test]
    fn popularity_saturates_at_the_scale() {
        assert!((popularity_points(0.30, 0.30) - 1.0).abs() < 1e-9);
        assert!((popularity_points(0.90, 0.30) - 1.0).abs() < 1e-9);
        assert!(popularity_points(0.03, 0.30) < popularity_points(0.10, 0.30));
    }

    #[test]
    fn rare_role_switches_to_the_habitual_role() {
        assert_eq!(
            data_role("SUPPORT", 13, 0.0, "MID", 427),
            ("MID".to_string(), true)
        );
        assert_eq!(
            data_role("BOT", 0, 0.0, "SUPPORT", 1033),
            ("SUPPORT".to_string(), true)
        );
    }

    #[test]
    fn enough_games_keep_the_requested_role() {
        assert_eq!(
            data_role("BOT", 120, 0.0, "SUPPORT", 1033),
            ("BOT".to_string(), false)
        );
    }

    #[test]
    fn a_class_that_plays_the_role_keeps_the_requested_role() {
        assert_eq!(
            data_role("JUNGLE", 11, 0.2, "TOP", 54),
            ("JUNGLE".to_string(), false)
        );
    }

    #[test]
    fn a_class_that_rarely_plays_the_role_switches_away() {
        assert_eq!(
            data_role("BOT", 0, 0.01, "SUPPORT", 1033),
            ("SUPPORT".to_string(), true)
        );
    }

    #[test]
    fn no_usable_primary_keeps_the_requested_role() {
        assert_eq!(
            data_role("TOP", 4, 0.0, "MID", 6),
            ("TOP".to_string(), false)
        );
    }

    #[test]
    fn hybrids_follow_the_attack_magic_gap() {
        assert_eq!(damage_profile("MIXTO", 5, 7), "AP");
        assert_eq!(damage_profile("MIXTO", 7, 5), "AD");
        assert_eq!(damage_profile("MIXTO", 6, 7), "HYB");
        assert_eq!(damage_profile("AD", 3, 9), "AD");
        assert_eq!(damage_profile("AP", 9, 3), "AP");
    }

    #[test]
    fn weights_sum_to_one_in_every_variant() {
        for stable in [true, false] {
            for classic in [true, false] {
                let w = score_weights(stable, classic);
                let total =
                    w.popularity + w.slot + w.winrate + w.defense + w.context + w.model + w.mode;
                assert!((total - 1.0).abs() < 1e-9, "{stable} {classic} {total}");
            }
        }
    }

    #[test]
    fn share_group_reports_share_and_slot() {
        let mut group = ShareGroup::default();
        group.add_games(100.0);
        group.add_item("3031", 40.0, 2.0);
        group.add_item("3031", 10.0, 4.0);
        assert!((group.share("3031") - 0.5).abs() < 1e-9);
        assert!((group.avg_slot("3031").unwrap() - 2.4).abs() < 1e-9);
        assert_eq!(group.share("3089"), 0.0);
        assert!(group.avg_slot("3089").is_none());
    }

    #[test]
    fn class_key_uses_every_tag_in_order() {
        let senna = vec!["Support".to_string(), "Marksman".to_string()];
        let thresh = vec!["Support".to_string(), "Tank".to_string()];
        assert_eq!(class_key(&senna), "Marksman|Support");
        assert_eq!(class_key(&thresh), "Support|Tank");
        assert_eq!(class_role_key("Support|Tank", "BOT"), "Support|Tank#BOT");
    }

    #[test]
    fn group_names_join_their_parts() {
        assert_eq!(
            group_name(&["classic", "SUPPORT", "Support", "AP"]),
            "classic|SUPPORT|Support|AP"
        );
        assert_eq!(group_name(&["aram", "*"]), "aram|*");
    }
}
