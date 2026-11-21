# RiftSense Rust

Reescritura en Rust del runtime de RiftSense: API Axum, motor de draft/build,
inferencia ONNX con `tract`, cliente LCU y lanzador de escritorio con WebView2.

El proyecto Python/Electron original **no está incluido** en este repositorio.
Solo se publica la versión Rust y los assets compartidos necesarios para
ejecutarla.

## Estado

| Componente | Estado |
| --- | --- |
| API HTTP (`/api/health`, meta, draft, bans, build, live, dataset, model, reload, update) | ✅ Rust, paridad 0 diferencias con Python |
| Motor de draft y build | ✅ Rust, misma lógica y pesos que `engine.py` |
| Modelos ML | ✅ ONNX + `tract-onnx` (sin Python en runtime) |
| Cliente LCU (estado, lockfile, localizador) | ✅ Rust; estado verificado contra Python |
| Mapper de champ select / partida en vivo | ✅ Portado; pendiente de probar con el cliente LoL abierto |
| Lanzador de escritorio | ✅ Rust + `tao`/`wry` (WebView2 en Windows) |
| Interfaz web | ⚠️ SPA compartida incluida en `web/`; pendiente reescritura en Rust |
| Actualizador | ✅ GitHub Releases + instalador Inno Setup |
| Pipeline de datos, scrapers y entrenamiento | ⏳ Pendiente; sigue en el proyecto original |

La paridad comprobada cubre un draft de ejemplo y una build de ejemplo:
12 recomendaciones, 10 objetos y análisis en vivo, con **0 diferencias**
frente a la API Python (incluidos textos y orden).

## Datos actuales

- Data Dragon **16.23.1** (parche **26.23**).
- **41.171** partidas válidas; **366** del parche actual (8 profesionales y
  358 Flex high elo de EUW1, KR, NA1 y LA1).
- Modelos reentrenados y exportados el **2026-11-21** con vocabulario de
  173 campeones y 205 objetos.

Resultados de rendimiento frente a la app original: [`PERFORMANCE.md`](PERFORMANCE.md).

## Instalación y actualizaciones

El instalador se publica en
[GitHub Releases](https://github.com/Fraandm/rift-sense-rs/releases). Instala
por usuario en `%LOCALAPPDATA%\Programs\RiftSense` e incluye binarios, modelos,
`data/processed` y `artifacts/meta.json`, así que no requiere Python.

Al arrancar, la app consulta la última release pública. Si hay una versión
nueva, muestra un aviso con el botón **Actualizar**: descarga el instalador,
lo ejecuta en modo silencioso y se reinicia. El instalador también se puede
usar manualmente.

Para compilar el instalador:

```powershell
.\installer\build-installer.ps1
```

## Estructura

```
riftsense-rs/
├── Cargo.toml / Cargo.lock
├── build.rs                   copia WebView2Loader.dll al target
├── src/
│   ├── main.rs                servidor CLI
│   ├── api.rs                 Axum + assets embebidos
│   ├── config.rs              detección de raíz y rutas
│   ├── data.rs                vocabulario, priors, meta
│   ├── engine.rs              draft, bans, build, meta
│   ├── model.rs               DraftNet/BuildNet vía tract
│   ├── lcu/
│   │   ├── locator.rs         búsqueda de LoL y lockfile
│   │   ├── client.rs          LCU HTTPS + Live Client API
│   │   └── mapper.rs          sesión LCU -> esquema RiftSense
│   └── bin/
│       ├── riftsense-desktop.rs   ventana nativa WebView2
│       └── parity_probe.rs        comparación con la API Python
├── web/                       SPA compartida (HTML/CSS/JS)
├── models/                    modelos ONNX + manifiesto
├── scripts/
│   └── build-windows.ps1      build release + DLL + modelos
├── PERFORMANCE.md
└── PORTING.md
```

## Requisitos

- `data/processed` y `artifacts/` del proyecto original (no incluidos).
- Rust stable (probado con Rust 1.100 y toolchain GNU/MinGW-w64).
- WebView2 Runtime (incluido en Windows 11) para el lanzador de escritorio.

Los modelos ONNX sí están incluidos en `models/`. El runtime no necesita
Python, PyTorch ni `.venv`.

Variables opcionales:

- `RIFTSENSE_ROOT`: raíz que contiene `data/processed` y `artifacts`.
- `RIFTSENSE_MODELS_DIR`: carpeta alternativa de los `.onnx`.
- `LOL_INSTALL_DIR` / `LOL_LOCKFILE`: ruta manual del cliente LoL.

## Ejecutar

Copia `data/processed` y `artifacts/` a la raíz del repo (o apunta
`RIFTSENSE_ROOT` al proyecto original) y ejecuta:

```powershell
cargo run --bin riftsense -- --port 8100
```

Lanzador de escritorio (WebView2):

```powershell
cargo run --bin riftsense-desktop
```

`build.rs` copia automáticamente `WebView2Loader.dll` (x64) junto al ejecutable.
El servidor de escritorio elige un puerto libre y abre la ventana; al cerrar la
ventana termina el proceso y el servidor.

## Compilar release

```powershell
.\scripts\build-windows.ps1
```

El script compila `riftsense.exe` y `riftsense-desktop.exe`, copia
`WebView2Loader.dll` junto al ejecutable y copia `models/` a `target/release/`.
Para ejecutar la release hay que llevar también `data/processed` y `artifacts`.

## Modelos ONNX

`models/draft_net.onnx` y `models/build_net.onnx` se exportaron desde los
checkpoints PyTorch del proyecto original. La validación contra PyTorch dio una
diferencia máxima de `5.96e-08` en draft y `2.86e-06` en build. El runtime usa
`tract-onnx`, por lo que la inferencia es 100 % Rust.

Para regenerar los modelos hace falta el proyecto original y su script de
exportación; no se incluye aquí.

## Paridad con Python

La paridad se verificó durante el portado con el servidor Python original en
`8000` y el Rust en `8100`: draft, build y live dieron **0 diferencias**. El
arnés de comparación y el proyecto Python no forman parte de este repositorio.

## Notas

- El motor Rust lee `data/processed` y `artifacts`; no necesita `data/raw`
  para recomendar.
- El cliente LCU es de solo lectura y usa el lockfile local; no envía acciones
  al juego.
- La búsqueda del cliente corre en segundo plano (hasta 45 s) y expone
  `searching`, `scannedDirs`, `currentDrive`, `startedAt`/`finishedAt` e
  `incomplete`, igual que el localizador Python. Fijar una ruta manual cancela
  la búsqueda en curso.
- La UI es la misma SPA compartida del proyecto original, incluida en `web/`
  para que el binario sea autocontenido. La reescritura de la UI a Rust queda
  pendiente.
- WebView2 es un componente del sistema; Electron/Chromium no está incluido.
