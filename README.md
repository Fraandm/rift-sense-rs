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
| Mapper de champ select / partida en vivo | ✅ Portado y probado con datos sintéticos de Grieta, ARAM, Mayhem y Arena |
| Modos de juego | ✅ ARAM, ARAM: Mayhem (KIWI) y Arena (CHERRY) con objetos y precios propios |
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

## Coach de compras en vivo

La pestaña **En vivo** lee el Live Client Data durante la partida y analiza
KDA, CS, nivel, diferencia de kills/CS/niveles entre equipos, campeones
enemigos fuertes y amenazas de curación. La siguiente compra se ajusta con ese
contexto:

- prioriza defensas si vas por detrás;
- recomienda antiheal si el rival tiene curación y aún no lo llevas;
- acelera objetos ofensivos si vas por delante;
- sugiere un componente asequible cuando el objeto completo no entra en oro.

Con **Auto** activo (por defecto), la lectura se actualiza cada 3 segundos
mientras la aplicación está visible. Sigue siendo solo lectura: no compra ni
envía inputs al juego.

## ARAM, ARAM: Mayhem y Arena

RiftSense detecta el modo con el `gameMode` del Live Client Data (`KIWI` para
**ARAM: Mayhem** y `CHERRY` para **Arena**), el mapa y la cola; en champ select
también usa la cola del LCU. La pestaña **En vivo** incluye un selector manual
de modo para probar builds sin partida.

- **ARAM**: filtra objetos que no existen en el mapa (Dark Seal, Cull, objetos
  de jungla, Guardián Ángel, Mejai, botas de nivel 3...). Sin recall, prioriza
  sustain, teamfight y compras al morir. El CS se muestra como referencia.
- **ARAM: Mayhem**: mismas reglas que ARAM, runas desactivadas y objetos
  exclusivos añadidos al motor: **Atma's Reckoning**, **Rite of Ruin** y
  **Sword of Blossoming Dawn**. Los augments no se exponen por la API local.
- **Arena**: no hay CS, roles ni fases de línea; se ocultan el draft y los
  componentes. Los objetos se compran completos con precios de Arena
  (starter y botas 500, legendario 2500) y se muestra el recordatorio de
  anvils (750 stats / 2000 legendario / 4000 prismático). La API local no
  identifica a los compañeros de trío ni expone vida de equipo o augments, así
  que el perfil rival usa a los demás jugadores y el estado se estima con KDA
  y nivel.

La base estadística sigue siendo de Grieta; en estos modos se aplican filtros
de legalidad, precios y contexto propios. La app no compra ni elige augments:
solo recomienda la siguiente compra.

## Ajustes y bandeja del sistema

La pestaña **Ajustes** guarda las preferencias en
`%APPDATA%\RiftSense\settings.json`:

- **Mostrar pestañas avanzadas**: oculta **Modelo** y **Datos** por defecto y
  las activa solo cuando quieres revisar métricas del motor.
- **Iniciar con Windows**: registra `RiftSense` en
  `HKCU\Software\Microsoft\Windows\CurrentVersion\Run` con el argumento
  `--minimized`, de modo que arranca directamente en la bandeja.
- **Minimizar a la bandeja**: al cerrar o minimizar la ventana, la app sigue
  activa en la bandeja. El icono abre la ventana y su menú permite **Salir**.
  Si está desactivado, cerrar la ventana termina el proceso.

El lanzador acepta `--minimized` (o `--tray`) para empezar oculto. Si el icono
de bandeja no se puede crear, la ventana se muestra igualmente para no dejar la
app inaccesible. La entrada de arranque se elimina al desactivar la opción o al
desinstalar.

## Instalación y actualizaciones

El instalador se publica en
[GitHub Releases](https://github.com/Fraandm/rift-sense-rs/releases). Instala
por usuario en `%LOCALAPPDATA%\Programs\RiftSense` e incluye binarios, modelos,
`data/processed` y `artifacts/meta.json`, así que no requiere Python.

Al arrancar, la app consulta la última release pública. Si hay una versión
nueva, muestra un aviso con el botón **Actualizar**: descarga el instalador,
verifica su SHA-256, lo ejecuta en modo silencioso y se reinicia. El
instalador también se puede usar manualmente.

Para compilar el instalador:

```powershell
.\installer\build-installer.ps1
```

Para firmar los binarios y el instalador con Authenticode:

```powershell
$env:RIFTSENSE_SIGN_CERT = "huella-o-ruta-al-pfx"
$env:RIFTSENSE_SIGN_PASSWORD = "..."   # solo si usas PFX
.\installer\build-installer.ps1
```

Sin certificado, el script avisa y genera un instalador sin firmar. También
puedes incluir el bootstrapper de WebView2 para instalaciones offline:

```powershell
.\installer\build-installer.ps1 -WebView2Bootstrapper "C:\ruta\MicrosoftEdgeWebview2Setup.exe"
```

El instalador comprueba WebView2 y, si falta, ejecuta el bootstrapper incluido
o muestra el enlace oficial de descarga.

Para actualizar datos y modelos sin reinstalar, sustituye `models/*.onnx` y
`data/processed` por la nueva exportación y pulsa **Recargar datos** en
Ajustes. El endpoint `/api/reload` valida el vocabulario y vuelve a cargar el
motor; si el modelo y el vocabulario no coinciden, conserva el anterior y
muestra el error.

## Estructura

```
riftsense-rs/
├── Cargo.toml / Cargo.lock
├── build.rs                   copia WebView2Loader.dll al target
├── src/
│   ├── main.rs                servidor CLI
│   ├── api.rs                 Axum + assets embebidos
│   ├── config.rs              detección de raíz y rutas
│   ├── settings.rs            preferencias, arranque y bandeja
│   ├── mode.rs                detección y reglas de ARAM/Mayhem/Arena
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

## Licencia y legal

El código, los binarios, los modelos y los datos empaquetados son propiedad de
RiftSense y se distribuyen para uso personal no comercial. Consulta
[`LICENSE`](LICENSE).

- [`THIRD_PARTY_NOTICES.md`](THIRD_PARTY_NOTICES.md): Riot Games, Leaguepedia,
  ONNX, WebView2, Inno Setup, fuentes y crates.
- [`PRIVACY.md`](PRIVACY.md): qué se guarda, qué se lee y qué sale a la red.
- [`SECURITY.md`](SECURITY.md): cómo reportar vulnerabilidades y el modelo de
  seguridad de la API local.
- [`docs/VALIDATION.md`](docs/VALIDATION.md): matriz de validación real y
  criterios de salida para 1.0.

RiftSense no está afiliado a Riot Games. No lee memoria del juego, no inyecta
código, no automatiza compras ni envía inputs.

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
El servidor de escritorio elige un puerto libre y abre la ventana. Con
**Minimizar a la bandeja** activo, cerrar la ventana solo la oculta; para
terminar el proceso usa **Salir** en el menú del icono de la bandeja o
desactiva esa opción en **Ajustes**.

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
