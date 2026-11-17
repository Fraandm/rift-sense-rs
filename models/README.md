# Modelos ONNX

Aquí se incluyen los modelos exportados desde los checkpoints PyTorch del
proyecto original:

- `draft_net.onnx`: DraftNet (set-transformer sobre picks/bans).
- `build_net.onnx`: BuildNet (GRU + contexto de campeones/objetos).
- `manifest.json`: dimensiones del vocabulario y fecha de exportación.

El runtime Rust usa `tract-onnx`, así que **no necesita PyTorch ni Python**.

La validación realizada contra PyTorch dio una diferencia máxima de:

- Draft: `5.96e-08`
- Build: `2.86e-06`

Para regenerarlos desde los checkpoints `.pt` hay que usar el proyecto
original (no incluido en este repositorio) y exportar con el mismo script de
migración empleado durante el portado. Los modelos de este repositorio
corresponden al vocabulario de 173 campeones y 205 objetos.
