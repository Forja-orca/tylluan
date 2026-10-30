# Informe de Implementación — Tarea AG-20d
**Diseño Final del Asistente de Instalación Estilo Clásico "Vieja Escuela" (Windows 2000 / XP)**  
**Agente:** `@antigravity`  
**Fecha:** 2026-10-30  
**Worktree:** `E:\tylluan-wt-antigravity`  
**Base commit:** `b9f1331` (origin/main)  
**Referencia directiva:** `docs/design/GUIA_UI_vieja_escuela.md` y revisión del Tech Lead T2243.

---

## 1. Resumen de Ajustes Finales (AG-20d)

En respuesta a la revisión T2243 del Tech Lead `@claude-code`, se completaron de forma rigurosa los cuatro puntos solicitados:

1. **Pantalla de Proceso (`pantalla_08_instalando_proceso.png`):**
   - El botón **`Siguiente >`** está estrictamente **desactivado** (`disabled`) durante la copia/instalación.
   - El botón **`Cancelar`** se encuentra **activo** para permitir abortar el proceso antes de la finalización.
   - El registro detallado permanece plegado bajo un botón de detalles técnicos.

2. **Pantalla de Nivel de Memoria (`pantalla_04_nivel_memoria.png`):**
   - Eliminadas siglas y jerga de la vista principal: sin menciones a `RAM`, `GPU`, `1024 dimensiones` ni parámetros en texto principal.
   - Nombres llanos y claros: **Ligera (Recomendada)**, **Equilibrada** y **Completa**.
   - Cada descripción tiene exactamente una frase de **<= 25 palabras** comprobadas:
     - *Ligera:* "Búsqueda rápida por palabras clave y conexiones de ideas. Máxima agilidad con consumo mínimo en cualquier equipo." (17 palabras).
     - *Equilibrada:* "Añade comprensión semántica intermedia para relacionar conceptos similares con bajo impacto en el procesador." (14 palabras).
     - *Completa:* "Máxima precisión de recuerdos y relaciones profundas. Recomendada para ordenadores potentes de trabajo continuo." (14 palabras).
   - Los parámetros reales (`--profile portable`, `--profile clinic`, `--profile server`) están encapsulados exclusivamente dentro del desplegable **"Mostrar detalles técnicos..."**.

3. **Garantías y Evidencias (`pantalla_07_evidencias_confianza.png` y bienvenida):**
   - Sustituida cualquier sobreafirmación por evidencias contrastables.
   - La fila de operación local refleja: **`Funciona en tu equipo por defecto`** ("Opera en local por defecto; tus recuerdos no salen de tu ordenador").
   - El texto introductorio de bienvenida define el producto con sobriedad como "sistema de memoria local para tus asistentes de inteligencia artificial".

4. **Serie Única y Coherente de Pantallas:**
   - Se eliminaron por completo las capturas intermedias anteriores.
   - La entrega consta de **una sola serie numerada de 12 archivos PNG** capturados a tamaño real (1366x768 viewport):
     - `pantalla_00_leeme_smartscreen_web.png`: Hoja previa informativa de descarga/LEEME (SmartScreen salta antes de ejecutar el binario).
     - `pantalla_01_bienvenida.png`: Bienvenida clásica con barra lateral de búho azul y aviso UAC.
     - `pantalla_02_tipo_instalacion.png`: Selección entre Instalación recomendada (un clic) o Personalizada.
     - `pantalla_03_carpeta_instalacion.png`: Selección de ruta de instalación (`--dir`).
     - `pantalla_04_nivel_memoria.png`: Selección de nivel llano (Ligera / Equilibrada / Completa) con parámetros ocultos en detalles.
     - `pantalla_05_opciones_asistentes.png`: Elección de asistentes a conectar (Claude Desktop / Cursor) y aviso de inicio automático como función futura de instalador.
     - `pantalla_06_resumen_previo.png`: Resumen claro de configuración antes de aplicar cambios.
     - `pantalla_07_evidencias_confianza.png`: Pantalla técnica "Por qué puedes fiarte" (MIT, SHA-256 en checksums.txt, construcción pública, opera en local).
     - `pantalla_08_instalando_proceso.png`: Progreso de instalación con barra segmentada, botón `Siguiente >` inactivo y `Cancelar` activo.
     - `pantalla_09_conectar_asistente.png`: Paso para conectar el asistente configurado con un clic.
     - `pantalla_10_instalacion_finalizada.png`: Pantalla de cierre exitoso con comando de inicio e instrucciones.
     - `pantalla_11_error_un_boton.png`: Pantalla de contingencia limpia con botón de acción única ("Reintentar con perfil ligero") y mensaje sin alarmismos.

---

## 2. Autochequeo de Reglas "Vieja Escuela"

Ejecutado sobre `docs/internal/asistente_instalacion_vieja_escuela.html`:
- `box-shadow`: Únicamente `none` (cero sombras en toda la hoja).
- `gradient`: 0 degradados (cero `linear-gradient` o fondos CSS degradados).
- `border-radius`: Máximo 3px (botones y marcos de 0px a 3px).
- `iconos decorativos / emojis`: 0 en todo el documento.
- `logo oficial`: Embebido en base64 desde `dashboard/public/tylluan-logo.jpg` (búho azul original sobre fondo blanco `#FFFFFF`).
- `paleta de color`: Fondo de ventana `#ECE9D8`, marco `#0A246A` / `#01285F`, texto `#000000`, superficie de contenido `#FFFFFF`.
- `tipografía`: `Tahoma`, `Segoe UI`, `Consolas` de sistema.

---

## 3. Ficheros que Componen la Entrega

Todos los ficheros permanecen en el worktree `E:\tylluan-wt-antigravity` **sin commitear**:
- `docs/internal/asistente_instalacion_vieja_escuela.html`: Maqueta interactiva HTML/CSS completa.
- `docs/internal/screenshots/pantalla_00_leeme_smartscreen_web.png`
- `docs/internal/screenshots/pantalla_01_bienvenida.png`
- `docs/internal/screenshots/pantalla_02_tipo_instalacion.png`
- `docs/internal/screenshots/pantalla_03_carpeta_instalacion.png`
- `docs/internal/screenshots/pantalla_04_nivel_memoria.png`
- `docs/internal/screenshots/pantalla_05_opciones_asistentes.png`
- `docs/internal/screenshots/pantalla_06_resumen_previo.png`
- `docs/internal/screenshots/pantalla_07_evidencias_confianza.png`
- `docs/internal/screenshots/pantalla_08_instalando_proceso.png`
- `docs/internal/screenshots/pantalla_09_conectar_asistente.png`
- `docs/internal/screenshots/pantalla_10_instalacion_finalizada.png`
- `docs/internal/screenshots/pantalla_11_error_un_boton.png`
- `docs/internal/IMPL_AG-20_asistente_instalacion_vieja_escuela_2026-10-30.md`: Este informe.

---

## 4. Impact
- **Ámbito:** Documentación y diseño de interfaz de instalación (`docs/internal/`). Cero modificaciones en crates Rust (`crates/`), código del dashboard en producción ni archivos de configuración compartidos.
- **Riesgo:** Nulo. No afecta a la compilación ni a los tests de la suite principal.

---

## 5. No Verificado
- No se ha compilado un instalador ejecutable nativo (`.exe` o WiX Toolset / InnoSetup); el trabajo comprende la especificación visual y estructural de las pantallas bajo la directiva de diseño clásico Windows 2000/XP.
- La página web de descarga previa o `LEEME.txt` donde vivirá la información de `pantalla_00` está maquetada dentro del archivo interactivo con fines demostrativos y no está aún desplegada en un servidor público.
