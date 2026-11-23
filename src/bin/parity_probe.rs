use anyhow::Result;
use riftsense::config::Paths;
use riftsense::engine::{BuildRequest, ChampionRef, DraftRequest, Engine, ItemRef, SideRef};
use serde_json::json;
use std::fs;

fn main() -> Result<()> {
    let paths = Paths::discover()?;
    println!("root: {}", paths.root.display());
    println!("models: {}", paths.models.display());
    let engine = Engine::load(paths)?;

    let draft_req = DraftRequest {
        role: "MID".to_string(),
        side: SideRef::Str("blue".to_string()),
        bans: ["Yone", "Aurora", "Rell", "Ambessa", "Poppy"]
            .iter()
            .map(|name| ChampionRef::Str(name.to_string()))
            .collect(),
        allies: vec![None, None, None, None, None],
        enemies: ["Gnar", "Vi", "Orianna", "Kai'Sa", "Nautilus"]
            .iter()
            .map(|name| Some(ChampionRef::Str(name.to_string())))
            .collect(),
        my_champion: None,
        limit: 12,
    };
    let draft = engine.draft_recommend(&draft_req)?;

    let build_req = BuildRequest {
        champion: ChampionRef::Str("Ahri".to_string()),
        role: "MID".to_string(),
        side: SideRef::Str("blue".to_string()),
        allies: ["Gnar", "Vi", "Ahri", "Kai'Sa", "Nautilus"]
            .iter()
            .map(|name| Some(ChampionRef::Str(name.to_string())))
            .collect(),
        enemies: ["Aatrox", "Sejuani", "Orianna", "Jinx", "Rell"]
            .iter()
            .map(|name| Some(ChampionRef::Str(name.to_string())))
            .collect(),
        owned: vec![ItemRef::Int(6655)],
        gold: 2500,
        minute: 14.0,
        game: None,
        mode: None,
        stable: false,
        limit: 10,
        simulate: true,
        target_item: None,
    };
    let build = engine.build_recommend(&build_req)?;

    let output = json!({
        "draft": draft,
        "build": build,
    });
    let text = serde_json::to_string_pretty(&output)?;
    let path = std::env::current_dir()?.join("parity_rust.json");
    fs::write(&path, &text)?;
    println!("escrito {}", path.display());
    println!(
        "draft: {} recomendaciones; build: {} objetos; notas: {:?}",
        output["draft"]["recommendations"]
            .as_array()
            .map(Vec::len)
            .unwrap_or(0),
        output["build"]["items"]
            .as_array()
            .map(Vec::len)
            .unwrap_or(0),
        engine.model_notes
    );
    Ok(())
}
