# Plan de portado a Rust

Mapa de los componentes del proyecto Python y su estado en `riftsense-rs`.

## Fase 1 — Runtime (completada)

| Original | Rust | Estado |
| --- | --- | --- |
| `lolanalyzer/config.py` | `src/config.rs` | ✅ |
| `lolanalyzer/utils.py` (`norm_name`) | `src/data.rs` | ✅ |
| `lolanalyzer/api/engine.py` | `src/engine.rs` | ✅ paridad 0 |
| `lolanalyzer/models/draft_net.py` | `scripts/export_onnx.py` + `src/model.rs` | ✅ ONNX/tract |
| `lolanalyzer/models/build_net.py` | `scripts/export_onnx.py` + `src/model.rs` | ✅ ONNX/tract |
| `lolanalyzer/api/schemas.py` | `src/engine.rs` (request structs) | ✅ |
| `lolanalyzer/api/server.py` | `src/api.rs`, `src/main.rs` | ✅ |
| `lolanalyzer/lcu/locator.py` | `src/lcu/locator.rs` | ✅ |
| `lolanalyzer/lcu/client.py` | `src/lcu/client.rs` | ✅ |
| `lolanalyzer/lcu/mapper.py` | `src/lcu/mapper.rs` | ✅ portado |
| `desktop/main.js` (ventana + servidor) | `src/bin/riftsense-desktop.rs` | ✅ Rust + WebView2 |
| `desktop/main.js` (updater) | `src/update.rs` + `installer/` | ✅ GitHub Releases + SHA-256 |
| `web/` | incluido en `web/` | ⚠️ SPA compartida, pendiente de portar |

## Fase 2 — Interfaz (pendiente)

La UI actual es una SPA de ~120 KB de JavaScript. Para completar la reescritura
en Rust hay dos opciones:

1. **Leptos/Dioxus + WASM** dentro de una ventana WebView2. Mantiene la estética
   y permite reutilizar el CSS actual.
2. **UI nativa** con `egui`/`iced`. Más trabajo de rediseño.

Recomendación: Leptos + `trunk`, sirviendo la app desde Axum y sustituyendo los
assets de `web/` por un frontend Rust.

## Fase 3 — Actualizador (completada)

- `reqwest` consulta la última release pública de GitHub.
- Descarga el instalador y verifica su SHA-256.
- Ejecuta Inno Setup en modo silencioso y termina el proceso.
- El instalador reemplaza los binarios y relanza `riftsense-desktop.exe`.
- Endpoints `/api/update/check` y `/api/update/install`; aviso en la SPA.

## Fase 4 — Datos y entrenamiento (pendiente)

| Original | Trabajo estimado |
| --- | --- |
| `lolanalyzer/scraper/leaguepedia.py` | Cliente Cargo API, paginación, caché |
| `lolanalyzer/scraper/ddragon.py` | Data Dragon + caché |
| `lolanalyzer/scraper/riot_api.py` | Cliente Riot + rate limits |
| `lolanalyzer/data/build_dataset.py` | Normalización, splits, vocabulario, priors |
| `lolanalyzer/models/train_draft.py` | Entrenamiento DraftNet |
| `lolanalyzer/models/train_build.py` | Entrenamiento BuildNet |
| `scripts/build_all.py` | Orquestación |

Opciones para el entrenamiento en Rust:

- `candle` (Meta) o `burn` para reimplementar las redes.
- Mantener el entrenamiento en Python y portar solo inferencia (estado actual).
- Exportar a safetensors/ONNX para paridad.

## Fase 5 — Empaquetado

- ✅ Instalador Inno Setup por usuario (`installer/build-installer.ps1`).
- ✅ `data/processed`, `artifacts`, modelos y binarios incluidos en el instalador.
- ⏳ Firma de código (el instalador es unsigned).
- ✅ Release `v0.2.0` con instalador y checksum.
- ⏳ CI de GitHub Actions (Windows + Rust + tests de paridad).

## Verificación

- `cargo check --all-targets`
- `cargo test --lib` (lockfile, ruta manual y escaneo con progreso)
- `cargo run --bin riftsense -- --port 8100`
- `cargo run --bin riftsense-desktop`
- Prueba manual de `riftsense-desktop.exe`.
- La paridad con Python se verificó durante el portado; el arnés no está incluido.
- La búsqueda LCU replica el hilo en segundo plano de Python: 45 s de límite,
  progreso por unidad y cancelación al fijar una ruta manual.
