# Changelog

## 1.8.0

Reentrenamiento con 160.885 partidas, modelos corregidos y remaster visual.

### Modelos

- **BuildNet**: inventario como conjunto (mover un objeto de ranura ya no
  cambia la recomendación), prior jerárquico campeón/rol/posición aprendido
  solo con entrenamiento y ponderación temporal de 180 días.
- En el test compartido (929 partidas, 48.190 objetivos), BuildNet pasa de
  top-1 23,0 % a **33,4 %**, top-3 46,3 % a **60,3 %** y top-5 58,3 % a
  **72,4 %**. La diferencia pareada de top-3 es **+13,9 puntos** (IC 95 %:
  +13,1 a +14,7), con bootstrap por partida.
- **DraftNet**: arquitectura factorizada con fortalezas por campeón y rol,
  sinergias y emparejamientos, calibrada con temperatura e intercepto.
  Test: AUC **0,567** y logloss **0,692**; la referencia publicada, corregida
  sobre las mismas partidas, daba 0,541 y 0,693. La mejora de logloss no es
  concluyente (±0,01) y el test es histórico, no prospectivo.
- Corregido el objetivo de DraftNet: la probabilidad es **P(gana el lado
  azul)**. El dataset también marcaba a todos los jugadores como rojos; ahora
  el lado, los aliados, los rivales y los winrates corresponden al jugador.
- ONNX reexportados y verificados contra PyTorch: error máximo `6,26e-07` en
  build y `5,96e-08` en draft, con lotes de 1, 2 y 7.

### Datos

- **160.885** partidas válidas, **924.095** builds, 173 campeones y 213 objetos.
- Importados todos los registros de los datasets de Kaggle; el ranked usa
  snapshots y timelines para ordenar inventarios en vez de truncar
  componentes, y ya no descarta 7 de cada 8 partidas.
- Particiones fijas por `GameId`: entrenamiento antes del 2026-09-15,
  validación hasta el 2026-10-14 y test desde el 2026-10-15. Las fechas
  estimadas nunca entran en validación ni en test.
- Deduplicación por fuente y `GameId`, rechazo de fechas futuras y validación
  estricta de campeones y objetos.
- Las compras truncadas por Riot se reconvierten desde caché en lugar de tomar
  los primeros componentes como build final.

### Interfaz

- Remaster **Command**: grafito y turquesa, jerarquía más clara y SVG
  estático. Sin frameworks nuevos, fuentes remotas, blur ni animación continua
  del overlay.
- La siguiente compra y el objetivo legendario aparecen antes del inventario;
  las runas aprovechan las columnas de escritorio.
- Con **Auto** activo, la app abre **En vivo** una vez por partida al
  detectarla, sin robar el foco y respetando cambios manuales y reconexiones.
- Navegación, contexto, guía rápida, foco y nombres accesibles mejorados;
  solo el equipo propio marca **TU ROL**.

### Verificación

- 1.384 builds en los cuatro modos con **0 incidencias** de legalidad.
- Pruebas Rust, clippy y formato; paridad ONNX; smoke visual con Playwright a
  1440, 1024, 768 y 390 px y prueba de navegación automática con LCU simulado.
- El overlay no se ha probado aún sobre una partida real: en pantalla completa
  exclusiva Windows puede ocultarlo; se recomienda ventana sin bordes.

## 1.7.0

Builds coherentes por modo: objetos legales en cada modo, evidencia real para
cada recomendación y menos sugerencias absurdas.

### Objetos por modo

- Arena usa sus propios IDs (p. ej. Rabadon's Deathcap 223089, Zhonya's
  223157, Liandry's Anguish 226653). Antes recomendaba objetos del Abismo y de
  la Grieta que no se compran en Arena.
- Los iniciales de Arena (Guardian's Orb, Hammer, Blade, Horn, Shroud, Amulet y
  Dirk) se reconocen por nombre: sus IDs también son propios de Arena.
- Los placeholders de anvil (`Legendary Mage Item`, etc.) y los Stat Bonus no
  aparecen como objetos recomendados.
- ARAM y Mayhem no reciben iniciales de Arena.
- Las botas de Arena se recomiendan con la misma regla que sus equivalentes de
  Grieta, a 500 de oro.
- Mayhem: Atma's Reckoning solo para campeones de AD; Rite of Ruin y Sword of
  Blossoming Dawn solo para AP. Sin partidas de Mayhem en la muestra, se usan
  los datos de ARAM.

### Builds en Grieta

- Un campeón con pocas partidas en su rol ya no recibe objetos de otra
  posición. Si su clase casi nunca juega ese rol (p. ej. Thresh en BOT), la
  build sigue su rol habitual y lo indica en las notas.
- La clase se calcula con todas sus etiquetas: Senna (Support y Marksman, juega
  BOT) no cuenta como Thresh (Support y Tank, juega soporte).
- Ya no aparecen objetos AD en campeones AP ni objetos AP en campeones AD sin
  datos propios. Yasuo de soporte pasa a su rol habitual (MID).
- La participación de cada objeto se suaviza hacia campeones parecidos cuando
  hay pocas partidas. Si no hay evidencia, el objeto no entra.
- El modelo de builds solo influye cuando el campeón tiene al menos 30 partidas
  en el rol: fuera de eso extrapolaba sin base.
- La puntuación ya no se normaliza contra la lista: muchos objetos no empatan
  en 96 y el orden refleja la participación real.

### Datos

- Nueva muestra de la Riot API por modo (ARAM 120 partidas, Arena 119, Mayhem
  sin partidas en la muestra de élite). Los objetos observados fijan qué se
  compra en cada modo.
- Los datos son de parches 15.x y 16.x. La muestra de ARAM y Arena es pequeña:
  las recomendaciones de campeones poco jugados se estiman con campeones
  parecidos y se indica en las notas.
- SoloQ de élite (359 partidas) se usa solo para validar. Solo 2 de 23
  combinaciones con 30 o más partidas coinciden en al menos la mitad de sus
  objetos con los priores profesionales; Grieta sigue usando los priores
  profesionales por su tamaño (41 000 partidas).

### Validación

- `scripts/validate-modes.py`: 1 384 builds (Grieta 865, ARAM 173, Mayhem 173,
  Arena 173). Ningún objeto fuera del mapa o sin compras reales en su modo, ni
  placeholders, ni duplicados.
- 48 tests Rust; `cargo fmt`, `clippy -D warnings`.
- Instalación local de 1.7.0: versión 1.7.0, parche 16.23.1, health `ok` y el
  actualizador responde que no hay versión nueva.

### Instalador

- `RiftSense-Setup-1.7.0.exe`, 27.873.667 bytes, SHA-256
  `e5cad22ddce5ab1f6f8f33aba6ee78c74ab13c6a4cd2f13b9886d6ec2b527a3e`.
- Sin firma Authenticode; SmartScreen puede avisar.

### Limitaciones conocidas

- Mayhem no tiene partidas propias en la muestra: sus objetos exclusivos son
  reglas por daño, no estadística.
- Grieta usa priores profesionales: en SoloQ las builds pueden diferir.

## 1.6.1

Probabilidad de victoria consistente entre jugadores del mismo equipo.

### Correcciones

- La probabilidad de Draft incluye el pick propio aunque la interfaz lo deje
  como fantasma para comparar alternativas. Antes, cada jugador evaluaba una
  partida con su propio hueco vacío y veía un porcentaje distinto.
- Todos los jugadores del mismo equipo ven el mismo valor con los mismos
  picks; el lado contrario ve el complementario (100% − el tuyo).
- La etiqueta pasa a **Prob. tu lado** y la ayuda explica la diferencia entre
  equipos y lados.
- Se mantiene la inclusión del pick propio en `draft_evaluate` y en la
  sinergia de las recomendaciones.

### Validación

- Mismo equipo, roles MID/BOT/SUPPORT con picks fantasma distintos: misma
  probabilidad (0.5533).
- Lados contrarios con los mismos picks: 0.5533 y 0.4467, suma 1.0.
- 33 tests Rust; `cargo fmt`, `clippy -D warnings` y `node --check`.

### Instalador

- `RiftSense-Setup-1.6.1.exe`, 27.678.666 bytes, SHA-256
  `67574b785800e7dd70cfb573bf2bc41ce218d54064a77f3f233a015b3e06f10e`.
- Sin firma Authenticode (no hay certificado configurado).
- Validado con instalación silenciosa, versión `1.6.1`, health `ok` y LCU
  conectado.

## 1.6.0

Runas recomendadas y correcciones de coherencia en las builds.

### Runas

- Nueva tarjeta **Runas recomendadas** en Draft para tu pick o para el mejor
  pick del rol.
- Página completa: árbol principal, keystone, tres runas primarias, árbol
  secundario, dos runas y tres fragmentos.
- Catálogo generado desde Data Dragon y páginas históricas de 359 partidas
  recientes (160 campeones, 272 combinaciones campeón/rol).
- Si no hay muestra suficiente, reglas por campeón, rol, clase y daño.
- Ajustes contra el rival: fragmento de armadura/RM según el perfil enemigo y
  **Derribado** contra dos o más tanques.
- La tarjeta indica si la página viene del histórico o de reglas, la muestra y
  avisa cuando es orientativa.

### Builds

- Los objetos deben encajar con el daño del campeón. Warwick ya no recibe
  Rylai's ni otros objetos AP puros; Kai'Sa, Gwen y los híbridos conservan sus
  objetos AP cuando tienen muestra propia.
- **Filo del Infinito** y **Rabadon** se penalizan como primera compra. En 33
  ADC probados, ninguno lo recomienda como primer objeto.
- La posición temporal de cada objeto se usa para no adelantar compras que
  suelen completarse más tarde.
- Las botas salen de la lista principal y se muestran solo en su tarjeta, sin
  competir con el primer objeto.
- El overlay usa la misma `nextPurchase` que la app, así que muestra el
  componente asequible y no el legendario de la lista.

### Validación

- 33 tests Rust; `cargo fmt`, `clippy -D warnings` y `node --check`.
- 33 ADC comprobados sin Filo del Infinito como primera compra.
- Warwick TOP/JUNGLE/MID/SUPPORT comprobado sin Rylai's.
- Tarjeta de runas probada en Chromium a 1440 px y 390 px.
- Overlay verificado contra la misma `nextPurchase` de la app.

### Instalador

- `RiftSense-Setup-1.6.0.exe`, 27.674.512 bytes, SHA-256
  `a497d8bd986dcd2b993a37c5ddec0446eaeb031a88c5f9900b38339fed738178`.
- Sin firma Authenticode (no hay certificado configurado).
- Validado con instalación silenciosa, versión `1.6.0`, health `ok` y LCU
  conectado.

## 1.5.1

Ajuste de precisión en antiheal y reducción de escudos.

### Correcciones

- El **antiheal** ya no se recomienda solo porque un campeón tenga una curación
  menor o esté por detrás. Se prioriza con curación por objetos, campeones
  dedicados (Soraka, Sona, Nami, Yuumi, Taric, etc.) o auto-curanderos fuertes.
- Un único **Eclipse** ya no activa Colmillo de Serpiente. La reducción de
  escudos requiere dos o más portadores, un escudo de equipo (Locket, Seraph's,
  Fimbulwinter, Gargoyle) o varios escudos apilados en el mismo rival.
- Las notas nombran la fuente: `Escudos relevantes: Jayce (Eclipse),
  Nocturne (Maw of Malmortius)` y `Curación por objetos: Nocturne (Maw...)`.
- La tarjeta **Builds rivales** solo muestra el chip de escudos cuando la señal
  es relevante y añade los campeones que curan cuando el antiheal está activo.
- Si la única señal es un auto-curandero flojo, el aviso de curación y la
  compra de antiheal desaparecen.
- Añadidos tests de prioridad de curación y escudos.

### Validación

- 28 tests Rust; `cargo fmt`, `clippy -D warnings` y `node --check`.
- Escenario real: Jayce con Eclipse + Sylas por detrás ya no genera antiheal ni
  Colmillo de Serpiente.
- Escenarios sintéticos: Locket activa la reducción de escudos; Soraka o un
  Sylas fuerte activan el antiheal.

### Instalador

- `RiftSense-Setup-1.5.1.exe`, 27.618.986 bytes, SHA-256
  `158fa2c9670395ed47ac28b39bad08eb8396d00aca2cdb9c64ebef889aad83ea`.
- Sin firma Authenticode (no hay certificado configurado).
- Validado con instalación silenciosa, versión `1.5.1`, health `ok` y LCU
  conectado.

## 1.5.0

Overlay compacto para jugar en una sola pantalla.

### Overlay

- Ventana transparente, sin bordes, siempre encima y **click-through** sobre
  League of Legends.
- Muestra solo lo necesario: siguiente compra, oro, oro que falta, botas
  recomendadas y un ajuste situacional.
- Indica **Build completa** cuando ya no hay compras pendientes.
- Aparece únicamente cuando el proceso del juego está en primer plano y se
  oculta al alt-tab.
- Se activa o desactiva en **Ajustes > Overlay en partida**, desde la bandeja
  del sistema o con **Ctrl+Shift+O**.
- El ajuste se guarda y se aplica al instante; por defecto queda activado.
- El overlay usa el mismo motor y datos que En vivo, pero sin abrir la ventana
  principal. No compra, no envía inputs y no captura el ratón.
- Probado con una partida real en ventana sin bordes: transparencia, posición,
  topmost, click-through, tamaño 392×132 y actualización cada 2,6 s.
- En pantalla completa exclusiva Windows puede impedir que una ventana normal
  se dibuje encima; se recomienda ventana sin bordes.

### Validación

- 26 tests Rust; `cargo fmt`, `clippy -D warnings` y `node --check`.
- Página del overlay probada en Chromium a 392×132 con datos reales y estados
  de espera, compra y build completa.
- Integración de escritorio probada: la ventana aparece al estar el juego en
  primer plano y se muestra/oculta al cambiar el ajuste.

### Instalador

- `RiftSense-Setup-1.5.0.exe`, 27.613.441 bytes, SHA-256
  `4c41b71b21cca871af44506e01cf47085d4233e5bc4305ea67c07541fc89ca29`.
- Sin firma Authenticode (no hay certificado configurado).
- Validado con instalación silenciosa, versión `1.5.0`, health `ok` y LCU
  conectado.

## 1.4.0

Análisis de builds rivales más profundo y visible en En vivo.

### Builds rivales

- Nueva tarjeta **Builds rivales** con una fila por enemigo: objetos, KDA, nivel,
  tipo de daño (AD/AP/mixto/tanque/soporte) y amenaza (normal/activo/fuerte).
- Se descartan consumibles, trinkets, mascotas de jungla y objetos desconocidos.
- Estadísticas por rival: armadura, resistencia mágica, vida, daño de ataque,
  poder de habilidad, crítico y velocidad de ataque.
- Resumen con resistencias totales, amenazas AD/AP, curación y escudos.
- Las notas señalan al rival concreto que acumula armadura, RM o vida.

### Recomendaciones

- La penetración de armadura/RM y el daño porcentual se activan también contra
  un único rival tanque, no solo cuando todo el equipo acumula resistencias.
- Las botas reaccionan a las builds reales: Plated Steelcaps contra autoataques
  y crítico, Mercury's Treads contra varias amenazas AP, y Sorcerer's Shoes
  contra resistencia mágica enemiga.
- Las compras situacionales evitan duplicar la misma categoría: por ejemplo,
  antiheal + penetración, antiheal + daño porcentual o escudos + daño porcentual.
- Los objetos situacionales se filtran por tipo de daño del campeón para no
  recomendar componentes AP a un AD ni componentes AD a un AP.
- Detección de curación y escudos ampliada (Eclipse, Seraph's, Lúpulo, Kaenic,
  Gargoyle, Banshee, Edge of Night, Redemption, Moonstone, Mikael, Helia,
  Unending Despair, etc.). Spirit Visage ya no cuenta por sí solo como curación.
- El antiheal y las notas nombran a los rivales que curan.

### Validación

- 25 tests Rust; `cargo fmt`, `clippy -D warnings` y `node --check`.
- Probado con una partida real de Grieta y con escenarios sintéticos de
  armadura, RM, vida, escudos, curación y autoataque.
- Tarjeta verificada en Chromium a 1440 px y 390 px sin desbordamiento ni
  errores de consola.

### Instalador

- `RiftSense-Setup-1.4.0.exe`, 27.602.515 bytes, SHA-256
  `76834d5623ffcb903e13e6960fd81b5e8627d9417f1ba38afa7eeab65a3aefc2`.
- Sin firma Authenticode (no hay certificado configurado).
- Validado con instalación silenciosa, versión `1.4.0`, health `ok` y LCU
  conectado.

## 1.3.0

Botas recomendadas y compras adaptadas a las builds rivales.

### Botas

- Nueva tarjeta **Botas recomendadas** en En vivo, separada de la lista
  principal para que no se pierda entre componentes.
- Elige botas según campeón, daño AP/AD, rol, tanque/support y perfil físico o
  mágico del rival.
- En MID, al completar la role quest, ofrece la mejora tier 3 gratuita.
- Si ya llevas botas, muestra las actuales; si no, permite comprarlas.
- En inventario lleno indica **Sin hueco** en lugar de esconder la
  recomendación.

### Builds rivales

- `En vivo` envía `enemyBuilds` desde el Live Client Data: campeón y objetos de
  cada rival.
- El motor analiza armadura, resistencia mágica, vida, crítico, velocidad de
  ataque, curación y escudos de las builds enemigas.
- Ajusta la recomendación con penetración de armadura/RM, daño porcentual,
  reducción de escudos, antiheal y defensas contra autoataques.
- Nueva tarjeta **Según las builds rivales** con hasta dos compras
  situacionales y el motivo.
- La lectura de partida muestra un resumen de las builds rivales.
- Ejemplo: contra mucha curación y armadura aparece Mortal Reminder; contra
  mucha vida, Liandry's/Demonic; contra escudos, Serpent's Fang.

### Validación

- La suite cubre asignación de roles y recetas.
- La selección de botas y el análisis de builds rivales se validaron con la API
  y con la interfaz en Chromium.

### Instalador

- `RiftSense-Setup-1.3.0.exe`, 27.572.332 bytes, SHA-256
  `8a3977e3877e067bb0fb362c4b3da2aaa2ab44d7559dfc86beadc1c963dea69d`.
- Sin firma Authenticode (no hay certificado configurado).
- Validado con instalación silenciosa, versión `1.3.0`, health `ok` y LCU
  conectado.

## 1.2.3

Detección del pick propio y roles rivales estimados.

### Draft y cliente

- El pick del jugador se detecta desde `myTeam`, `championPickIntent` y las
  acciones del champ select, incluido el hover.
- En Grieta, el pick detectado se muestra como **pick fantasma** en su rol y
  sigue dejando el hueco libre para comparar alternativas.
- El campeón detectado se excluye de las recomendaciones y de la prioridad de
  bans, aunque el hueco se envíe vacío al motor.
- La tarjeta del cliente muestra el campeón detectado en vez de “Sin campeón
  seleccionado”.
- El contador de picks conocidos incluye el pick detectado.

### Roles rivales

- El cliente no expone los roles del equipo rival. Se estiman con una
  asignación conjunta por probabilidad de rol de cada campeón, no con
  `cellId % 5`.
- La asignación conjunta evita que dos flex picks ocupen el mismo rol.
- Los roles estimados se marcan con **≈** y un aviso en Draft y En vivo.
- Si el cliente sí expone los roles (por ejemplo, en partida), se respetan.
- Tests nuevos para la asignación conjunta de roles.

### Instalador

- `RiftSense-Setup-1.2.3.exe`, 27.533.549 bytes, SHA-256
  `369cc3cab743ebfad498bf369f5b03796160fc5d03dcb369f2a9ccd175f22c6d`.
- Sin firma Authenticode (no hay certificado configurado).
- Validado con instalación silenciosa, versión `1.2.3`, health `ok` y LCU
  conectado.

## 1.2.2

El objetivo legendario se mantiene al empezar la compra.

### Build en vivo

- Al comprar un componente del legendario recomendado (por ejemplo, Espadón
  para Filo del Infinito), el objetivo **ya no desaparece** ni cambia por el
  nuevo ranking.
- El objetivo se envía a `/api/live` como `targetItem` y el backend lo mantiene
  mientras sea legal, no esté completado y siga siendo el objetivo del jugador.
- Si el objeto se completa, o cambia el campeón, rol o modo, el objetivo se
  libera y se elige el siguiente.
- Las alternativas siguen visibles y el objetivo fijado aparece siempre en la
  tarjeta **Objetivo legendario**.
- El objetivo se limpia al terminar la partida para no arrastrarlo a la
  siguiente.

### Instalador

- `RiftSense-Setup-1.2.2.exe`, 27.530.970 bytes, SHA-256
  `c1110bd5a111249ca5212ae8233afb784cb096a7724d004c87240acf6a72c983`.
- Sin firma Authenticode (no hay certificado configurado).
- Validado con instalación silenciosa, versión `1.2.2`, health `ok` y LCU
  conectado.

## 1.2.1

Confianza de la recomendación revisada en ARAM.

### Build en vivo

- La confianza ya no se degrada cuando la build está completa: se muestra
  **Build completa** en lugar de un porcentaje bajo sin siguiente objeto.
- Se reconoce el modelo aunque la respuesta use `recommendationMode`, no solo
  `mode: model`.
- La muestra se calcula con el mejor dato de los tres primeros candidatos y de
  la siguiente compra, no solo con el primer objeto.
- Se añade la claridad de la recomendación (diferencia con la segunda opción).
- La penalización de ARAM es menor y se explica como **datos de Grieta
  adaptados**, en lugar de tratarlo como un modo sin datos.
- Los componentes se consideran material de mejora solo si forman parte real
  de un legendario legal; Doran's o botas ya no impiden marcar la build como
  completa.
- Con contexto completo (modelo, 5 rivales, partida en vivo y muestra amplia),
  la confianza de ARAM sube a **Alta** en lugar de quedarse en 44%.

### Instalador

- `RiftSense-Setup-1.2.1.exe`, 27.525.332 bytes, SHA-256
  `9bbe7e61af51ae625a1a9ac79ea7c258510f53bb48e462021523ee796d946d22`.
- Sin firma Authenticode (no hay certificado configurado).
- Validado con instalación silenciosa, versión `1.2.1`, health `ok` y LCU
  conectado.

## 1.2.0

Objetivo legendario fijo en la build en vivo.

### Build en vivo

- Nueva tarjeta **Objetivo legendario**: fija el siguiente objeto completo y
  muestra dos alternativas, incluso cuando el inventario está lleno de
  componentes.
- Calcula qué componentes del inventario se reutilizan, el coste real de
  completar el legendario y cuánto oro falta; indica si libera huecos.
- El botón **Completar** consume los componentes y descuenta solo el coste de
  mejora cuando se usa el inventario manual.
- El inventario cuenta correctamente componentes duplicados (por ejemplo, dos
  Needlessly Large Rod).
- Si hay un objetivo asequible se prioriza; si no, se muestra el mejor con el
  oro que falta.
- `nextLegendary` y `legendaryOptions` se añaden a `/api/live` y `/api/build`.
- Tests nuevos para recetas anidadas y componentes duplicados.

### Notas

- La build ya no se marca como completa solo porque haya seis componentes;
  primero busca un legendario que los reutilice.

### Instalador

- `RiftSense-Setup-1.2.0.exe`, 27.525.228 bytes, SHA-256
  `35abcedf974007b6667a27cfa58944baf045c16b258d308abd1a642835265c72`.
- Sin firma Authenticode (no hay certificado configurado).
- Validado con instalación silenciosa, versión `1.2.0`, health `ok` y LCU
  conectado.

## 1.1.0

Reconocimiento de modo en Draft, resumen post-partida y correcciones manuales.

### Draft inteligente

- Selector de modo **Grieta / ARAM / Mayhem / Arena** con detección automática
  desde el LCU (`queueId`, `gameMode` y mapa).
- En ARAM, Mayhem y Arena desaparecen los roles, el lado y los bans: la
  composición se muestra como `A1–A5` / `R1–R5` sin intentar encajar campeones
  en roles de Grieta.
- Arena muestra **Tu trío** con 3 huecos.
- Importar un champ select no clásico ya no está bloqueado; se importa la
  composición tal cual.

### Resumen post-partida

- Nueva pestaña **Resumen** con build real, objetos comprados, seguimiento de
  la recomendación, KDA, duración, augments/anviles manuales y resultado.
- Seguimiento en vivo: cada objeto nuevo se compara con la compra recomendada
  y con el plan; se guarda en `localStorage` (máximo 20 partidas).
- Al terminar la partida se consulta el historial local del LCU
  (`/api/client/postgame`) para obtener resultado, inventario final, KDA y
  augments detectados.
- Si la app no siguió la partida, el resumen importa la última partida del
  historial del cliente y evita duplicados por `gameId`.
- La sesión en vivo se reanuda tras recargar la app y conserva las compras ya
  registradas.
- Resultado manual **Victoria / Derrota** si el LCU no lo devuelve.
- Borrar el historial no vuelve a importar la misma partida; las partidas
  nuevas siguen entrando.

### Correcciones manuales Arena / Mayhem

- **Arena:** elegir trío (3 huecos), augments y anviles usados.
- **Mayhem:** registrar hasta 4 augments manuales.
- Los augments/anviles se muestran en las notas de partida y se guardan en el
  resumen. La API local sigue sin exponerlos.

### Backend

- Nuevo endpoint `GET /api/client/postgame` con parser tolerante del historial
  de partidas del LCU (identidad por PUUID/summonerId, items, KDA, augments).
- `map_champ_select` coloca el pick propio primero y no usa roles en modos no
  clásicos.
- Tests nuevos para el parser postgame y el orden de equipos.

### Instalador

- `RiftSense-Setup-1.1.0.exe`, 27.508.758 bytes, SHA-256
  `3d4fb8986af4c19335de38425cb604065c0ade43124561ca9b570befa0e489f5`.
- Sin firma Authenticode (no hay certificado configurado).
- Validado con instalación silenciosa, versión `1.1.0`, health `ok` y LCU
  conectado.

## 1.0.2

Mejora de interfaz, accesibilidad y experiencia en vivo.

### Interfaz en vivo

- La lista de compras muestra **4 alternativas** y un botón **Ver N alternativas**
  para desplegar el resto, en lugar de una lista larga siempre abierta.
- La tarjeta de lectura de partida ya no duplica la próxima compra; el CTA
  principal queda en la lista de recomendaciones.
- En móvil hay una **barra fija de próxima compra** con precio y botón de
  compra, accesible sin bajar hasta el final de la página.
- La lista solo se vuelve a pintar cuando cambia el contenido relevante, para
  no perder el foco del teclado durante la actualización automática.

### Accesibilidad y feedback

- Las imágenes rotas se gestionan con un listener global y `data-fallback`;
  se eliminan los `onerror` inline que bloqueaba la CSP.
- Los avisos duran 3,6 s, se limita la pila a 2, no bloquean clics y se limpian
  al cambiar de pestaña.
- `aria-live` se limita a un resumen de la próxima compra, sin anunciar toda
  la lista cada 3 segundos.
- `Auto` expone `aria-pressed`; los slots de composición tienen nombre
  accesible; el botón de quitar objeto de inventario es visible en táctil y
  mide 26 px.
- Se aclaran los textos secundarios (`--muted-2`) y se suben los tamaños
  mínimos de metadatos y etiquetas.

### Responsive y calidad

- Sin scroll horizontal en 390, 768 y 1440 px; ajustes y tarjetas corregidos
  para móvil.
- Barra móvil por encima de la zona segura y avisos por encima de la barra.
- `build.rs` vigila `web/` para que la SPA embebida se recompile al cambiar.
- Validación con Playwright: 0 errores de consola/CSP, navegación y compra
  móvil comprobadas.

### Limitaciones conocidas

- La API local de Riot no expone la ranura 8 de role quest.
- El instalador de 1.0.2 se publica sin firma Authenticode si no se compila con
  un certificado.
- La validación en partidas reales de Grieta/ARAM/Mayhem/Arena sigue pendiente
  de la matriz documentada en `docs/VALIDATION.md`.

## 1.0.1

Parche de inventario por rol y reentrenamiento del modelo.

### Inventario por rol

- **Bot:** tras completar la role quest dispone de 6 objetos + botas en una
  ranura dedicada (7 piezas). La interfaz muestra 7 huecos y reserva el ultimo
  para las botas.
- **Support:** `World Atlas`, `Runic Compass`, `Bounty of Worlds` y las cinco
  mejoras finales entran en el vocabulario y en BuildNet. La cadena se
  recomienda paso a paso y no se ofrece a otros roles.
- **Mid:** si lleva botas tier 2 y la role quest esta completada, se ofrece la
  mejora tier 3 gratuita como sustitucion, sin ocupar una ranura extra.
- **Jungla:** las mascotas de jungla solo se recomiendan al jungla.
- **Doran + soporte:** no se recomienda Doran al support porque no puede
  convivir con el objeto de soporte.
- El objeto de soporte ocupa una ranura normal. La ranura de role quest del
  support es para wards de control y no se contabiliza como objeto de build.

### Modelo y datos

- Vocabulario de objetos ampliado con los 8 objetos de la quest de soporte.
- `roleBoundItem` de Riot se incorpora a la secuencia de entrenamiento para
  conservar las botas del bot cuando el cliente las mueve a la ranura 8.
- Dataset reconstruido con Leaguepedia y partidas high elo de la Riot API:
  **41.211 partidas**, **412.000 builds** y **213 objetos**.
- BuildNet reentrenado y exportado a ONNX: validacion top1 **30,3 %** y top3
  **55,3 %**; test temporal top1 **22,5 %** y top3 **44,0 %** (el split cambio
  al anadir partidas nuevas, no es directamente comparable con 1.0.0).

### Interfaz

- Inventario de 7 huecos para el bot, con etiqueta de role quest.
- Las mejoras de soporte y la mejora de botas de mid se muestran como
  "Mejorar", no como una compra normal.
- Si el cliente local no expone la ranura 8, RiftSense conserva las ultimas
  botas vistas y las mantiene en la septima ranura.

### Limitaciones conocidas

- La API local de Riot no expone la ranura de role quest. La finalizacion se
  infiere a partir del minuto 16 y de las botas vistas; el cliente puede
  completarla antes.
- El instalador de 1.0.1 se publica sin firma Authenticode si no se compila con
  un certificado. El build acepta `RIFTSENSE_SIGN_CERT`.
- La validacion en partidas reales de Grieta/ARAM/Mayhem/Arena sigue pendiente
  de la matriz documentada en `docs/VALIDATION.md`.

## 1.0.0

Primera versión estable.

### Recomendaciones

- Compra principal estable basada en **BuildNet** y winrate histórico.
- Probabilidad del modelo y WR visibles en cada objeto.
- El contexto de partida ya no sustituye la compra principal.
- Antiheal mostrado como sugerencia situacional.
- Componentes asequibles y plan de build.
- ARAM, ARAM: Mayhem y Arena con filtros, precios y limitaciones propios.
- Poro-Snax, consumibles, trinkets, anvils y vouchers fuera del inventario de build.
- Objetos Doran excluidos en ARAM/Mayhem.

### Aplicación

- Instancia única y restauración desde bandeja.
- Arranque con Windows y minimizado a bandeja.
- Ajustes persistentes y pestañas avanzadas opcionales.
- Logs persistentes, diagnóstico copiable y apertura de logs.
- Frescura del dataset y recarga de datos/modelos.
- Guía de bienvenida.
- Actualizador con SHA-256 y comando de instalación en la carpeta activa.
- Comprobación de WebView2 y firma Authenticode opcional.

### Seguridad

- Host y Origin locales obligatorios en la API.
- CSP, `X-Frame-Options`, `X-Content-Type-Options` y `Referrer-Policy`.
- Sin CORS permisivo.
- Documentación de privacidad, seguridad y terceros.

### Calidad

- GitHub Actions con fmt, clippy, tests, build y sintaxis JS.
- 14 tests unitarios.
- Script de validación de instalador: hash, firma, instalación, health y desinstalación.
- Matriz de validación real por modo documentada.

### Limitaciones conocidas

- El instalador de 1.0.0 se publica sin firma Authenticode si no se compila con
  un certificado. El build acepta `RIFTSENSE_SIGN_CERT`.
- La matriz de partidas reales de Grieta/ARAM/Mayhem/Arena está documentada y
  debe completarse con partidas del usuario; las pruebas de esta versión usan
  datos sintéticos y smoke tests de instalación.
- La actualización de datos/modelos es manual (sustituir archivos y recargar).
  El canal remoto de paquetes de modelos queda para una versión posterior.
