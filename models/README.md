# Modelos ONNX

Aquí se incluyen los modelos exportados desde los checkpoints PyTorch del
proyecto original:

- `draft_net.onnx`: DraftNet (set-transformer sobre picks/bans).
- `build_net.onnx`: BuildNet (GRU + contexto de campeones/objetos).
- `manifest.json`: dimensiones del vocabulario y fecha de exportación.

El runtime Rust usa `tract-onnx`, así que **no necesita PyTorch ni Python**.

Validación actual (2026-11-21, Data Dragon 16.23.1):

- Draft: diferencia máxima `0.0`
- Build: diferencia máxima `3.81e-06`, mismo argmax

La API Rust contra la API Python también se verificó con **0 diferencias** en
draft, build y live.

Para regenerarlos desde los checkpoints `.pt` hay que usar el proyecto
original (no incluido en este repositorio) y exportar con el mismo script de
migración empleado durante el portado. Los modelos de este repositorio
corresponden al vocabulario de 173 campeones y 205 objetos y al parche 26.23
(16.23.1).
