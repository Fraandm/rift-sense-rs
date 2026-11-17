use anyhow::{Context, Result};
use std::path::Path;
use std::sync::Arc;
use tract_onnx::prelude::*;

/// Envoltorio de un modelo ONNX ejecutado por tract (100% Rust).
pub struct OnnxModel {
    plan: Arc<dyn Runnable>,
}

impl OnnxModel {
    pub fn load(path: &Path) -> Result<Self> {
        let plan = tract_onnx::onnx()
            .model_for_path(path)
            .with_context(|| format!("No se pudo leer el ONNX {}", path.display()))?
            .into_optimized()
            .with_context(|| format!("No se pudo optimizar {}", path.display()))?
            .into_runnable()
            .with_context(|| format!("No se pudo compilar {}", path.display()))?;
        let plan: Arc<dyn Runnable> = Arc::new(plan);
        Ok(Self { plan })
    }

    fn run(&self, inputs: TVec<TValue>) -> Result<TVec<TValue>> {
        self.plan.run(inputs).context("Fallo de inferencia ONNX")
    }
}

pub struct DraftModel {
    model: OnnxModel,
}

impl DraftModel {
    pub fn load(path: &Path) -> Result<Self> {
        Ok(Self {
            model: OnnxModel::load(path)?,
        })
    }

    /// Devuelve la probabilidad de victoria del equipo azul para cada fila.
    pub fn predict(
        &self,
        n: usize,
        champ: &[i64],
        role: &[i64],
        side: &[i64],
        kind: &[i64],
        mask: &[bool],
    ) -> Result<Vec<f32>> {
        let champ = tract_ndarray::Array2::from_shape_vec((n, 20), champ.to_vec())?;
        let role = tract_ndarray::Array2::from_shape_vec((n, 20), role.to_vec())?;
        let side = tract_ndarray::Array2::from_shape_vec((n, 20), side.to_vec())?;
        let kind = tract_ndarray::Array2::from_shape_vec((n, 20), kind.to_vec())?;
        let mask = tract_ndarray::Array2::from_shape_vec((n, 20), mask.to_vec())?;
        let output = self.model.run(tvec!(
            champ.into_tensor().into(),
            role.into_tensor().into(),
            side.into_tensor().into(),
            kind.into_tensor().into(),
            mask.into_tensor().into()
        ))?;
        let view = output[0]
            .to_plain_array_view::<f32>()
            .context("Salida DraftNet no es f32")?;
        Ok(view
            .iter()
            .map(|value| 1.0 / (1.0 + (-value).exp()))
            .collect())
    }
}

pub struct BuildModel {
    model: OnnxModel,
}

impl BuildModel {
    pub fn load(path: &Path) -> Result<Self> {
        Ok(Self {
            model: OnnxModel::load(path)?,
        })
    }

    /// Devuelve la probabilidad softmax de cada objeto (incluido el indice 0).
    pub fn predict(
        &self,
        champ: i64,
        role: i64,
        side: i64,
        ally: &[i64; 5],
        enemy: &[i64; 5],
        items: &[i64; 7],
        slot: i64,
    ) -> Result<Vec<f32>> {
        let champ = tract_ndarray::Array1::from_vec(vec![champ]);
        let role = tract_ndarray::Array1::from_vec(vec![role]);
        let side = tract_ndarray::Array1::from_vec(vec![side]);
        let ally = tract_ndarray::Array2::from_shape_vec((1, 5), ally.to_vec())?;
        let enemy = tract_ndarray::Array2::from_shape_vec((1, 5), enemy.to_vec())?;
        let items = tract_ndarray::Array2::from_shape_vec((1, 7), items.to_vec())?;
        let slot = tract_ndarray::Array1::from_vec(vec![slot]);
        let output = self.model.run(tvec!(
            champ.into_tensor().into(),
            role.into_tensor().into(),
            side.into_tensor().into(),
            ally.into_tensor().into(),
            enemy.into_tensor().into(),
            items.into_tensor().into(),
            slot.into_tensor().into()
        ))?;
        let view = output[0]
            .to_plain_array_view::<f32>()
            .context("Salida BuildNet no es f32")?;
        let values: Vec<f32> = view.iter().copied().collect();
        let max = values.iter().copied().fold(f32::NEG_INFINITY, f32::max);
        let exps: Vec<f32> = values.iter().map(|value| (value - max).exp()).collect();
        let sum: f32 = exps.iter().sum();
        Ok(exps.into_iter().map(|value| value / sum).collect())
    }
}
