//! Verifica paridad numerica PyTorch/tract con fixtures y lotes variables.
use anyhow::{Context, Result, ensure};
use riftsense::model::{BuildModel, DraftModel};
use serde::Deserialize;
use std::{fs, path::PathBuf};

#[derive(Deserialize)]
struct BuildInputs {
    champ: Vec<i64>,
    role: Vec<i64>,
    side: Vec<i64>,
    ally: Vec<[i64; 5]>,
    enemy: Vec<[i64; 5]>,
    items: Vec<[i64; 7]>,
    slot: Vec<i64>,
}

#[derive(Deserialize)]
struct DraftInputs {
    champ: Vec<Vec<i64>>,
    role: Vec<Vec<i64>>,
    side: Vec<Vec<i64>>,
    kind: Vec<Vec<i64>>,
    mask: Vec<Vec<bool>>,
}

#[derive(Deserialize)]
struct Case<T> {
    inputs: T,
    expected: Vec<f32>,
}

#[derive(Deserialize)]
struct Fixtures {
    build: Vec<Case<BuildInputs>>,
    draft: Vec<Case<DraftInputs>>,
}

fn compare(actual: &[f32], expected: &[f32]) -> Result<f32> {
    ensure!(actual.len() == expected.len(), "Forma de salida incorrecta");
    let mut maximum = 0.0_f32;
    for (a, b) in actual.iter().zip(expected) {
        ensure!(a.is_finite() && b.is_finite(), "Salida no finita");
        maximum = maximum.max((a - b).abs());
    }
    ensure!(maximum < 0.00003, "Error de paridad: {maximum}");
    Ok(maximum)
}

fn main() -> Result<()> {
    let mut args = std::env::args().skip(1);
    let models = PathBuf::from(args.next().context("Falta carpeta models")?);
    let fixtures_path = args.next().context("Falta archivo de fixtures")?;
    let fixtures: Fixtures = serde_json::from_slice(&fs::read(fixtures_path)?)?;
    let build = BuildModel::load(&models.join("build_net.onnx"))?;
    let draft = if fixtures.draft.is_empty() {
        None
    } else {
        Some(DraftModel::load(&models.join("draft_net.onnx"))?)
    };
    let mut max_build = 0.0_f32;
    let mut max_draft = 0.0_f32;
    for case in &fixtures.build {
        let x = &case.inputs;
        let actual = build.predict(
            x.champ[0],
            x.role[0],
            x.side[0],
            &x.ally[0],
            &x.enemy[0],
            &x.items[0],
            x.slot[0],
        )?;
        max_build = max_build.max(compare(&actual, &case.expected)?);
    }
    for case in &fixtures.draft {
        let x = &case.inputs;
        let flatten = |v: &[Vec<i64>]| v.iter().flatten().copied().collect::<Vec<_>>();
        let mask = x.mask.iter().flatten().copied().collect::<Vec<_>>();
        let actual = draft.as_ref().context("Falta DraftNet")?.predict(
            x.champ.len(),
            &flatten(&x.champ),
            &flatten(&x.role),
            &flatten(&x.side),
            &flatten(&x.kind),
            &mask,
        )?;
        max_draft = max_draft.max(compare(&actual, &case.expected)?);
    }
    println!(
        "{}",
        serde_json::json!({
            "ok": true,
            "buildCases": fixtures.build.len(),
            "draftCases": fixtures.draft.len(),
            "maxBuildError": max_build,
            "maxDraftError": max_draft,
        })
    );
    Ok(())
}
