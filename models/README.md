# Modelos ONNX

Modelos exportados desde los checkpoints PyTorch reentrenados el 2026-11-23:

- `draft_net.onnx`: DraftNet factorizado (fortalezas campeón/rol, sinergias y
  emparejamientos) con calibración de temperatura e intercepto; objetivo
  explícito **P(gana el lado azul)**.
- `build_net.onnx`: BuildNet con contexto de campeón, rol, aliados, rivales y
  ranura; historial como conjunto (invariante al orden) y prior jerárquico
  campeón/rol/posición aprendido solo con entrenamiento.
- `manifest.json`: vocabulario, arquitectura, hashes del dataset y fecha de
  exportación.
- `quality-report.json`: métricas de validación/test, comparación con 1.7.0 y
  límites conocidos.

El runtime Rust usa `tract-onnx`, así que **no necesita PyTorch ni Python**.

Paridad actual con PyTorch (`model_quality_probe`, 2026-11-23):

- Draft: diferencia máxima `5.96e-08`.
- Build: diferencia máxima `6.26e-07`.
- Lotes de draft de tamaño 1, 2 y 7; builds en distintos puntos del inventario.

La API Rust contra la API Python también se verificó con **0 diferencias** en
draft, build y live.

Los modelos corresponden al vocabulario de 173 campeones y 213 objetos y al
parche 26.23 (16.23.1). Para regenerarlos hacen falta los checkpoints `.pt` del
proyecto de datos y `scripts/export_onnx.py`.
