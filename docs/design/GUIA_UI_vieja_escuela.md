# Guía de interfaz de Tylluan: "vieja escuela" (v1, 2026-10-30)

Decisión de José: está cansado del estilo web de IA y prefiere cualquier estilo de la vieja escuela. Esta guía manda sobre cualquier skill genérica de diseño. Aplica al instalador, al dashboard y a todo lo que vea una persona.


## Qué analicé
- Skills de Claude Code instaladas: `frontend-design` (oficial), `frontend-design-direction`, `design-system`, `accessibility` y `frontend-a11y` (ECC), `design-critique`, `accessibility-review`, `ux-copy`, `design-system` del plugin de diseño, `theme-factory`, `canvas-design`, `brand-guidelines`.
- Skill propia de Antigravity (`E:\ForjaMCPo3\skills\frontend-sovereign-ui-skill.md`): prescribe shadcn/ui, Inter, violeta (#7c3aed), esquinas de 10 y 14 px e iconos Lucide. **Es exactamente el estilo que José rechaza: queda retirada para Tylluan.**
- Lo aprovechable de las demás: la lista de "señales de página generada" que debe evitarse (`frontend-design`: resaltar una sola palabra del titular, etiquetas en mayúsculas, rótulos innecesarios sobre el contenido, marcadores numerados decorativos, animaciones de entrada dispersas); el método de contraste y teclado de `accessibility` (WCAG 2.2 AA); y los principios de `ux-copy`.
- No verificado: que Antigravity pueda cargar las skills de Claude Code; por eso las reglas esenciales van escritas aquí.

## El estilo, con reglas comprobables
1. **Tipografía:** solo fuentes del sistema (Tahoma, Segoe UI, Verdana; Georgia para texto largo; Consolas para datos). Nada de Google Fonts ni Inter, Poppins, Space Grotesk. Tamaño pequeño y fijo: 12 a 13 px de interfaz, 11 px secundario.
2. **Color:** grises neutros (paneles #ECECEC o #F0F0F0, contenido blanco, texto #000 a #222). Un solo acento, el azul del logo (muéstralo muestreado del logo, no inventado). Verde, ámbar y rojo solo para estados. Sin degradados, sin transparencias, sin brillos ni neón, sin modo oscuro por defecto.
3. **Forma:** esquinas de 0 a 3 px como máximo. Bordes visibles de 1 px (#808080 a #A0A0A0). Botones y campos con relieve clásico (borde claro arriba-izquierda, oscuro abajo-derecha) o planos con borde. Sin `box-shadow`. Sin píldoras, sin tarjetas flotantes, sin barras de color en el lateral de un bloque.
4. **Composición:** alineado a la izquierda, denso, con tablas, `fieldset` con `legend` y etiquetas junto al campo. Nada de héroe centrado ni de grandes espacios en blanco. Un máximo de dos niveles de títulos.
5. **Iconos:** ninguno decorativo. Botones con texto. Si un icono es imprescindible, 16 por 16, monocromo y funcional. Cero emojis. Cero ilustraciones.
6. **Movimiento:** ninguno salvo el estado normal de una barra de progreso. Sin fundidos ni "skeleton". Respetar `prefers-reduced-motion`.
7. **Texto:** español llano, frases cortas, verbos en imperativo, sin siglas, puertos ni rutas salvo la carpeta elegida. Error = qué pasó + qué hacer. Nada se afirma si no está medido (ver "Verdad").
8. **Accesibilidad (WCAG 2.2 AA):** contraste 4,5:1 medido, foco visible (puntos de 1 px, estilo clásico), todo con teclado, objetivos de al menos 24 px, etiquetas reales en cada campo.
9. **Logo:** el real, sin modificar: `dashboard/public/tylluan-logo.jpg` (640×640, búho azul sobre fondo blanco). Como es un JPG sobre blanco, el fondo del contenedor del logo es blanco.

## Verdad en los textos
Ninguna cifra ni afirmación sin comprobar. Ejemplo corregido de la primera maqueta de AG-20: decía que "Microsoft cobra miles de euros al año". No está verificado; se sustituye por "Tylluan es libre y gratuito y no paga una firma comercial".

## Nivel exigido
Software profesional de la época de Windows 2000 y XP: alineación al píxel, espaciado en múltiplos de 4, todos los estados cubiertos (normal, pasar el ratón, pulsado, desactivado, foco), nada recortado a 1366×768 ni a 1920×1080.

## Comprobaciones que haré yo sobre cada entrega
- `grep` del CSS y el HTML: cero `gradient`, cero `box-shadow` (salvo `none`), cero `border-radius` mayor de 3 px, cero emojis, tipografía solo del sistema.
- Contraste medido con herramienta, no a ojo.
- Capturas a tamaño real, propias, de cada pantalla.
- Cuestionario de salida (si alguna respuesta es sí, se rehace): ¿lo habría generado una plantilla de shadcn? ¿tiene esquinas redondeadas, degradado, sombra, píldora, emoji, Inter o un héroe centrado?
