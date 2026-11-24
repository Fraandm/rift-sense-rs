# RiftSense Command · remaster de interfaz

## Dirección visual

Centro de mando oscuro: superficies grafito, acento turquesa y dorado reservado
para economía. Jerarquía tipográfica más clara, bordes definidos, menos brillos
y una barra lateral más estrecha. La geometría del draft es SVG estático.

Se conservan las vistas, IDs y eventos de la aplicación. No hay framework nuevo,
fuentes descargadas, librerías de animación ni imágenes de fondo pesadas.

## Cambios funcionales de presentación

- Barra de contexto con sección actual, acceso a la guía y aviso de solo lectura.
- «Siguiente compra» y objetivo legendario antes del inventario y detalles rivales.
- Runas junto a la composición en pantallas amplias; flujo vertical en móvil.
- Estados activos, teclado, enlace para saltar al contenido y controles táctiles.
- Solo el equipo propio marca «TU ROL»; antes se resaltaba también el mismo rol rival.
- Los nombres accesibles de los slots incluyen su texto visible y el rol.
- Con Auto activo se abre En vivo al detectar `GameStart`, una partida en curso
  o Live Client Data. Solo una vez por partida; los sondeos y reconexiones no
  anulan cambios manuales posteriores. No enfoca la ventana ni interrumpe el juego.
- Overlay con fondo legible, sin blur ni pulso continuo; conserva transparencia
  exterior y `pointer-events: none`.

## Rendimiento

La nueva capa `web/assets/css/remaster.css` pesa aproximadamente 17 KB sin
comprimir / 4,7 KB gzip (el servidor puede servirla sin compresión).
No solicita Google Fonts. Se reutiliza un único `Intl.NumberFormat` para
formatear cifras. No se añadieron timers, nuevos sondeos ni trabajo por frame.

El test verifica ausencia de animaciones infinitas, tanto con movimiento normal
como con movimiento reducido. Esto no es una medición de FPS del juego ni del
consumo de la ventana nativa durante una partida.

## Validación reproducible

Con el servidor de desarrollo en marcha:

```powershell
python scripts/test-interface.py --url http://127.0.0.1:5279
```

Requiere Playwright. En Windows utiliza Edge instalado, sin descargar otro
navegador. Se puede elegir otro canal con `--channel`.

Comprueba:

- Draft, En vivo, Resumen, Cliente y Ajustes a 1440, 1024, 768 y 390 px.
- Sin desbordamientos horizontales.
- Cuatro modos de juego y los tres aliados de Arena.
- Búsqueda de campeón, cierre por teclado y guía rápida.
- Ejemplos reales de draft y build, sin automatización del juego.
- Sin excepciones JavaScript, fuentes remotas ni animaciones continuas.
- Transparencia, ausencia de blur y de captura de puntero en el overlay web.

Capturas e informe se escriben en `dist/interface-check/`.
La auditoría Lighthouse de la vista Draft inicial obtuvo **100** en accesibilidad,
buenas prácticas y SEO, sin hallazgos. No sustituye una auditoría manual completa
ni prueba el overlay sobre pantalla completa exclusiva.
