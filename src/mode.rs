//! Deteccion y reglas de los modos de juego soportados.

/// Modos con comportamiento propio dentro de RiftSense.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GameMode {
    /// Grieta del Invocador: draft, roles y builds de linea.
    Classic,
    /// ARAM clasico en cualquiera de sus mapas.
    Aram,
    /// ARAM: Mayhem (KIWI): ARAM con augments y objetos exclusivos.
    Mayhem,
    /// Arena (CHERRY): rondas, anvils y objetos completos.
    Arena,
}

impl GameMode {
    pub fn from_explicit(value: &str) -> Option<Self> {
        match compact(value).as_str() {
            "classic" | "clasico" | "sr" | "grieta" | "summonersrift" | "rift" => {
                Some(Self::Classic)
            }
            "aram" | "howlingabyss" | "abismo" => Some(Self::Aram),
            "mayhem" | "arammayhem" | "kiwi" | "caos" => Some(Self::Mayhem),
            "arena" | "cherry" | "rings" => Some(Self::Arena),
            _ => None,
        }
    }

    pub fn from_game_mode(value: &str) -> Option<Self> {
        match compact(value).as_str() {
            "classic" | "practice" | "practicetool" | "tutorial" | "urf" | "oneforall"
            | "nexusblitz" | "ultbook" => Some(Self::Classic),
            "aram" => Some(Self::Aram),
            "kiwi" => Some(Self::Mayhem),
            "cherry" => Some(Self::Arena),
            _ => None,
        }
    }

    pub fn from_map(value: &str) -> Option<Self> {
        let key = compact(value);
        let key = key.strip_prefix("map").unwrap_or(&key);
        match key {
            "11" => Some(Self::Classic),
            "12" | "35" | "36" => Some(Self::Aram),
            "30" => Some(Self::Arena),
            _ => None,
        }
    }

    pub fn from_queue(queue: i64) -> Option<Self> {
        match queue {
            450 => Some(Self::Aram),
            2400 => Some(Self::Mayhem),
            1700 | 1710 | 1715 | 1720 | 1750 => Some(Self::Arena),
            _ => None,
        }
    }

    /// Resuelve el modo con la informacion disponible, dando prioridad a lo
    /// que el cliente declara de forma explicita.
    pub fn resolve(
        explicit: Option<&str>,
        game_mode: Option<&str>,
        map_name: Option<&str>,
        queue_id: Option<i64>,
    ) -> Self {
        if let Some(mode) = explicit.and_then(Self::from_explicit) {
            return mode;
        }
        if let Some(mode) = queue_id.and_then(Self::from_queue) {
            return mode;
        }
        if let Some(mode) = game_mode.and_then(Self::from_game_mode) {
            if mode != Self::Classic {
                return mode;
            }
        }
        if let Some(mode) = map_name.and_then(Self::from_map) {
            return mode;
        }
        game_mode
            .and_then(Self::from_game_mode)
            .unwrap_or(Self::Classic)
    }

    pub fn key(self) -> &'static str {
        match self {
            Self::Classic => "classic",
            Self::Aram => "aram",
            Self::Mayhem => "mayhem",
            Self::Arena => "arena",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Classic => "Grieta del Invocador",
            Self::Aram => "ARAM",
            Self::Mayhem => "ARAM: Mayhem",
            Self::Arena => "Arena",
        }
    }

    pub fn short_label(self) -> &'static str {
        match self {
            Self::Classic => "Grieta",
            Self::Aram => "ARAM",
            Self::Mayhem => "Mayhem",
            Self::Arena => "Arena",
        }
    }

    pub fn description(self) -> &'static str {
        match self {
            Self::Classic => "Draft y builds de Summoner's Rift.",
            Self::Aram => {
                "Una calle, sin recall y con oro pasivo. Prioriza sustain y compras al morir."
            }
            Self::Mayhem => {
                "ARAM con augments, runas desactivadas y objetos exclusivos. Compra al morir."
            }
            Self::Arena => {
                "Combates por rondas: compra objetos completos, elige augments y usa anvils."
            }
        }
    }

    pub fn notes(self) -> &'static [&'static str] {
        match self {
            Self::Classic => &[],
            Self::Aram => &[
                "ARAM: sin recall; compra al morir o al volver a base.",
                "El oro es pasivo y el CS compartido: prioriza spikes de equipo y sustain.",
            ],
            Self::Mayhem => &[
                "ARAM: Mayhem: runas desactivadas y hasta cuatro augments por partida.",
                "Objetos exclusivos disponibles: Atma's Reckoning, Rite of Ruin y Sword of Blossoming Dawn.",
                "La API local no expone los augments elegidos: la recomendacion usa campeon, rivales, oro e inventario.",
            ],
            Self::Arena => &[
                "Arena: compra objetos completos entre rondas; no hay componentes en la tienda.",
                "Precios de Arena: starters y botas 500; legendarios 2500; anvils 750 (stats), 2000 (legendario) y 4000 (prismatico).",
                "El anvil prismatico gratis llega en la ronda 3. La API local no expone augments ni vida de equipo.",
            ],
        }
    }

    pub fn is_classic(self) -> bool {
        self == Self::Classic
    }

    pub fn uses_roles(self) -> bool {
        self == Self::Classic
    }

    pub fn uses_cs(self) -> bool {
        self != Self::Arena
    }

    pub fn uses_team_stats(self) -> bool {
        self != Self::Arena
    }

    pub fn uses_phase(self) -> bool {
        self != Self::Arena
    }

    pub fn uses_components(self) -> bool {
        self != Self::Arena
    }

    pub fn max_enemies(self) -> usize {
        match self {
            Self::Arena => 20,
            _ => 5,
        }
    }
}

fn compact(value: &str) -> String {
    value
        .trim()
        .to_lowercase()
        .chars()
        .filter(|character| character.is_ascii_alphanumeric())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects_live_modes() {
        assert_eq!(
            GameMode::resolve(None, Some("KIWI"), Some("Map12"), None),
            GameMode::Mayhem
        );
        assert_eq!(
            GameMode::resolve(None, Some("CHERRY"), Some("Map30"), None),
            GameMode::Arena
        );
        assert_eq!(
            GameMode::resolve(None, Some("ARAM"), Some("Map12"), None),
            GameMode::Aram
        );
        assert_eq!(
            GameMode::resolve(None, Some("CLASSIC"), Some("Map11"), None),
            GameMode::Classic
        );
    }

    #[test]
    fn queue_and_map_fallbacks() {
        assert_eq!(
            GameMode::resolve(None, None, None, Some(2400)),
            GameMode::Mayhem
        );
        assert_eq!(
            GameMode::resolve(None, None, None, Some(450)),
            GameMode::Aram
        );
        assert_eq!(
            GameMode::resolve(None, None, None, Some(1700)),
            GameMode::Arena
        );
        assert_eq!(
            GameMode::resolve(None, Some("CLASSIC"), Some("Map12"), None),
            GameMode::Aram
        );
        assert_eq!(
            GameMode::resolve(None, Some("CLASSIC"), Some("Map30"), None),
            GameMode::Arena
        );
    }

    #[test]
    fn explicit_mode_wins() {
        assert_eq!(
            GameMode::resolve(Some("arena"), Some("CLASSIC"), Some("Map11"), None),
            GameMode::Arena
        );
        assert_eq!(
            GameMode::resolve(Some("mayhem"), None, None, None),
            GameMode::Mayhem
        );
    }
}
