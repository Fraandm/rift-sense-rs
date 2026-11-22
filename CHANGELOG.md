# Changelog

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
