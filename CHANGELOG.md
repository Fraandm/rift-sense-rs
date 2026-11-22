# Changelog

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
