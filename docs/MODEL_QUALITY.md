# Auditoría y mejora de los modelos

## Errores corregidos

- **Lado de las builds:** el constructor reutilizaba la variable de un bucle
  anterior y marcaba todos los jugadores como rojos. Se calcula ahora el lado
  de cada jugador; esto también corrige sus aliados, enemigos y winrates.
- **Objetivo de DraftNet:** `winner_side = 0` significa victoria azul, pero se
  entrenaba BCE con ese cero y se presentaba la salida como P(azul). El objetivo
  es ahora explícitamente `blue_win`. El exportador corrige también checkpoints
  históricos sin ese marcador.
- **Inventarios:** una ranura no es una fecha de compra. Kaggle usa la primera
  aparición observable de cada objeto en sus snapshots/timeline. En Riot se
  ordena el inventario final por sus compras, en vez de truncar las seis primeras
  compras de componentes ya consumidos.
- **Fechas sintéticas:** los parches normalizados 25.x/26.x ya no se interpretan
  como versiones Riot 15.x/16.x. Las fechas estimadas no entran en validación o
  test; tampoco se adelantan parches posteriores al entrenamiento.
- **Partidas repetidas:** se prioriza una fuente por GameId, no se unen plantillas
  repetidas por tener alias de jugador diferentes.
- **Selección de draft:** se minimiza logloss, apropiada para probabilidades,
  en lugar de seleccionar exclusivamente por AUC.

## Protocolo de comparación

Particiones temporales fijas, con la partida completa como unidad:

| Partición | Fechas reales |
| --- | --- |
| Entrenamiento | Antes de 2026-09-15 |
| Selección | Desde 2026-09-15 hasta 2026-10-14 |
| Test | Desde 2026-10-15 |

Para draft, la segunda mitad de validación se reserva a calibración; la primera
selecciona modelos. Los datos sin bans pueden entrenar los candidatos habilitados,
con tokens enmascarados; no modifican el conjunto de evaluación comparable.
Los hiperparámetros y checkpoints se seleccionan sin leer el test.

Se compara contra los checkpoints publicados en 1.7.0, sobre **las mismas
partidas, vocabulario y etiquetas corregidas**. Los informes antiguos no son
directamente comparables. Para draft se informa tanto de la salida publicada
como de esa misma red con la dirección azul/rojo corregida.

Los intervalos de diferencias se calculan por bootstrap de partidas, no de
jugadores ni de cada objeto por separado. El test es histórico y ya se había
utilizado en trabajo anterior: no sustituye una validación prospectiva.

## Candidatos

- BuildNet secuencial con prior jerárquico aprendido solo del train.
- BuildNet con inventario como conjunto, invariante a cambios de ranura.
- Variante con decaimiento temporal para reducir el peso de metas antiguos.
- Baseline fuerte campeón/rol/posición, además del modelo publicado.
- DraftNet transformer pequeño regularizado.
- DraftNet aditivo campeón/rol, con y sin partidas de bans desconocidos.
- DraftNet factorizado, con interacciones de sinergia y emparejamientos.
- Calibración de temperatura e intercepto integrada en el ONNX de draft.

Los candidatos y sus informes viven en el proyecto de datos:
`artifacts/quality-candidates/`. El entrenamiento no sobrescribe producción.
`quality_comparison.json` contiene la selección, métricas pareadas y SHA-256
del dataset, vocabulario y checkpoints seleccionados.

## Verificación del runtime

`model_quality_probe` compara las probabilidades de PyTorch con `tract`:
builds en distintos puntos del inventario, drafts parciales, bans enmascarados
y lotes de tamaños 1, 2 y 7. Se exige error absoluto menor de `3e-5`.

Después se ejecutan las pruebas Rust y el barrido de legalidad por los cuatro
modos. La aplicación sigue siendo únicamente de lectura.

## Resultados en el test compartido

Mismas partidas, vocabulario y etiquetas para todos los candidatos. Build: 929
partidas y 48.190 objetivos. Draft: 799 partidas comparables.

| Modelo | Métrica | 1.7.0 corregido | 1.8.0 |
| --- | --- | ---: | ---: |
| BuildNet | top-1 | 0,2304 | **0,3343** |
| BuildNet | top-3 | 0,4635 | **0,6027** |
| BuildNet | top-5 | 0,5833 | **0,7241** |
| BuildNet | MRR | 0,3945 | **0,5057** |
| DraftNet | AUC | 0,5409 | **0,5674** |
| DraftNet | logloss | 0,6931 | 0,6924 |
| DraftNet | accuracy | 0,5507 | 0,5219 |

- BuildNet: diferencia pareada de top-3 **+0,1392**; IC 95 % por bootstrap de
  partidas **[0,1314; 0,1468]**.
- DraftNet: diferencia de logloss **−0,0007**; IC 95 % **[−0,0113; 0,0092]**.
  No es concluyente. La accuracy baja porque la calibración nueva reparte las
  probabilidades de forma distinta; el AUC sí mejora.
- La red publicada en 1.7.0, tal como se desplegó, obtenía AUC 0,4591 por el
  objetivo invertido. La columna «1.7.0 corregido» reexporta esos mismos pesos
  con la dirección azul correcta, que es la comparación justa.

## Límites

- Un inventario final ordenado por primera aparición **no** es un historial
  completo de compras: faltan objetos vendidos y compras entre snapshots.
- Los winrates de objetos son descriptivos y tienen sesgo de duración y ventaja;
  no demuestran el efecto causal de comprar un objeto.
- Picks/bans por sí solos no contienen habilidad, estado de forma o coordinación.
  La probabilidad de draft sigue siendo orientativa.
- Se debe recoger un nuevo holdout futuro antes de afirmar mejoras prospectivas.
