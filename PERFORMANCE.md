# Comparativa de rendimiento: Python/Electron vs Rust/WebView2

Fecha: 17 nov 2026  
Equipo: AMD Ryzen 5 3600 (6C/12T), 16 GB RAM, Windows 11 Pro  
Datos y modelos: los mismos `data/processed`, `artifacts` y cargas para ambos.

## Metodología

- Backend aislado: servidor Python/FastAPI y servidor Rust/Axum, sin UI.
- Escritorio: app original (Electron + Python) y app Rust (`tao`/`wry` + WebView2).
- 20 s de estabilización, 15 s de reposo.
- 10.000 `GET /api/health`, 300 `POST /api/draft`, 200 `POST /api/build`, secuenciales.
- Medición de CPU/RAM/I/O con `psutil` sobre todo el árbol de procesos.
- MB = MiB. CPU % sistema = porcentaje sobre los 12 hilos; “núcleos” = CPU % sistema × 12.
- No es un test de concurrencia máxima: simula el uso de la UI, una petición detrás de otra.
- La UI es la misma SPA en ambos; la comparativa mide runtime, motor y shell de escritorio.

## Espacio en disco

| Componente | Python/Electron | Rust/WebView2 |
|---|---:|---:|
| Ejecutable principal | 105,6 MB | **34,7 MB** |
| WebView2Loader | — | 0,2 MB |
| Modelos ONNX | — | 2,8 MB |
| `.venv` | 716,3 MB | — |
| Python base | 342,9 MB | — |
| `data/processed` + `artifacts` | 9,2 MB | 9,2 MB |
| Código web/backend | 0,7 MB | embebido en el `.exe` |
| **Runtime instalado** | **1.174,8 MB** | **46,9 MB** |
| Extracción temporal al arrancar | 357,3 MB | **0 MB** |
| **Runtime en ejecución** | **1.532,1 MB** | **46,9 MB** |
| Caché de UI | 16,3 MB (AppData) | 16,1 MB (WebView2) |

- **25,0× menos espacio instalado** y **32,7× menos en ejecución**.
- Rust elimina `.venv` y Python base; no extrae 357 MB a `%TEMP%` en cada arranque.
- WebView2 Runtime es un componente del sistema (Windows 11); Electron/Chromium va incluido en el `.exe` original.

## Arranque

Mediana de 3 arranques completos:

| Métrica | Python/Electron | Rust/WebView2 | Mejora |
|---|---:|---:|---:|
| Backend listo (frío) | 2,178 s | **0,107 s** | **20,4×** |
| Lanzador → UI completa | **9,572 s** | **0,49 s** | **19,5×** |
| Servidor listo dentro del escritorio | 2,9 s* | 0,09 s | ~32× |
| Extracción portable | ~6,6 s | 0 s | — |

\* Medido desde que arranca Electron en el log, no desde el doble clic.

El arranque original se reparte aproximadamente en: extracción del portable (~6,6 s) + Electron/servidor/UI (~3,0 s). El Rust arranca el servidor en ~0,09 s y pinta la UI en ~0,49 s, sin extracción.

## Backend aislado

| Métrica | Python/FastAPI | Rust/Axum | Diferencia |
|---|---:|---:|---:|
| CPU en reposo | 0,02 % | 0,00 % | ~0 |
| RAM en reposo (RSS) | 267,9 MB | **32,0 MB** | **8,4× menos** |
| RAM privada | 600,8 MB | **18,1 MB** | **33,2× menos** |
| Health: latencia media | 1,129 ms | **0,503 ms** | **2,2× más rápido** |
| Health: peticiones/s | 885,5 | **1.986,5** | **2,2×** |
| Health: CPU | 6,01 % | 0,83 % | 7,2× menos |
| Draft: latencia media | **9,454 ms** | 9,951 ms | Python 5 % más rápido |
| Draft: peticiones/s | **105,8** | 100,5 | ~igual |
| Draft: CPU | 49,35 % | **7,81 %** | **6,3× menos** |
| Build: latencia media | 16,640 ms | **2,761 ms** | **6,0× más rápido** |
| Build: peticiones/s | 60,1 | **362,2** | **6,0×** |
| Build: CPU | 49,49 % | **6,84 %** | **7,2× menos** |

En CPU, 49,35 % del sistema equivale a **5,92 núcleos**; 7,81 % equivale a **0,94 núcleos**. El draft de Rust es prácticamente igual de rápido usando **un solo núcleo**, mientras PyTorch reparte el trabajo entre casi 6.

## Escritorio completo

| Métrica | Python + Electron | Rust + WebView2 | Diferencia |
|---|---:|---:|---:|
| CPU en reposo | 0,03 % | 0,02 % | ~0 |
| RAM en reposo (RSS) | 650,0 MB | **403,0 MB** | **1,6× menos** (247 MB) |
| RAM privada | 1.072,4 MB | **336,2 MB** | **3,2× menos** (736 MB) |
| Health: latencia | 1,006 ms | **0,502 ms** | **2,0×** |
| Health: peticiones/s | 993,8 | **1.990,5** | **2,0×** |
| Health: CPU | 6,00 % | 0,85 % | 7,1× menos |
| Draft: latencia | **8,382 ms** | 9,841 ms | Python 17 % más rápido |
| Draft: CPU | 49,55 % | **7,81 %** | **6,3× menos** |
| Build: latencia | 14,681 ms | **2,722 ms** | **5,4× más rápido** |
| Build: CPU | 49,31 % | **6,46 %** | **7,6× menos** |

La capa de UI pesa parecido en ambos: Electron añade ~382 MB RSS y WebView2 ~371 MB. La diferencia de RAM viene sobre todo del backend Python (268 MB) frente al Rust (32 MB) y de las dependencias.

## Lectura de los resultados

- **Rust gana claramente en arranque, espacio, RAM, health y build.**
- **El draft tiene latencia casi idéntica**, con Rust ligeramente más lento en escritorio (9,84 ms vs 8,38 ms), pero usando **~6,3× menos CPU**. La causa es que PyTorch paraleliza en ~6 núcleos y `tract` usa ~1. Si se paralelizara la inferencia Rust, probablemente también ganaría en latencia.
- **El build es donde Rust más destaca**: 5,4–6,0× más rápido y 7× menos CPU.
- **El disco deja de ser un problema**: 47 MB de runtime frente a 1,17 GB instalados y 1,53 GB en ejecución con la extracción portable.
- **La UI no es el factor dominante**: ambas shells pesan ~370–380 MB; el gran ahorro está en eliminar Python, PyTorch y la extracción del portable.

## Reproducir

Las mediciones se hicieron con el arnés de desarrollo sobre el proyecto
original; ese arnés y la app Python no forman parte de este repositorio. La
metodología está descrita arriba y los resultados resumidos en este documento.

## Limitaciones

- Peticiones secuenciales, no concurrencia máxima.
- Caché de Windows caliente; un arranque tras reiniciar puede leer más disco.
- La UI es la misma SPA; no se compara un frontend Rust nativo.
- El arranque original se midió con las actualizaciones desactivadas.
- Diferencias de latencia de ±5–10 % entre ejecuciones; los arranques son mediana de 3.
- WebView2 Runtime es del sistema; en un Windows sin WebView2 habría que instalarlo.
