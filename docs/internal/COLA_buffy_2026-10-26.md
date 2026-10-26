# COLA Buffy — 2026-10-26 (la relaya Jose) — revision roja del README

Contexto: README publico reescrito (rama local `readme-pro-2026-10-25`, SIN empujar). Leelo con `git show readme-pro-2026-10-25:README.md`. Jose quiere nivel "de los mas grandes, 100% honesto, 0 basura". DSH ya hizo una auditoria de afirmaciones (T-DSH-91) y Hermes ejecuta los comandos (H-35). Tu angulo es DISTINTO: red-team.

## M-BUFFY-11 — leelo como un atacante y como un escetico (SOLO LECTURA)
1. Seguridad: para CADA frase de la seccion "Security" y de las advertencias, intenta romperla con codigo (grep/lectura/tests en proceso con `test_state`, sin lanzar kernel en 47004): ¿hay un camino donde sea falsa, exagerada o incompleta? Ejemplos a atacar: "binds to 127.0.0.1 by default", "token ... in the directory the kernel was started from", la lista publica de rutas sin token, "state-changing REST routes reject non-local Origin", "Guilds run with your user's rights", "coherence gate filters every recall".
2. Diagramas: abre `docs/assets/how-it-fits.svg` y `memory-flow.svg` (en la rama) y comprueba cada caja/flecha contra el codigo: ¿existe esa relacion tal como se dibuja? (p. ej. "Guards" antes de "Memory", "Other instances" solo opcional).
3. Lo que falta: ¿hay algo que un usuario nuevo NECESITA saber para no hacerse dano y que el README no dice? (borrado de datos, donde se guarda la DB, como parar el kernel, que pasa si pierde el token).
4. Entrega: tabla linea | frase o elemento | intento de romperlo (comando/evidencia) | resultado | texto corregido propuesto. Max 20 filas por gravedad. Informe `IMPL_M-BUFFY-11_*.md` con `## Impact` y "No verificado", y resumen `[COLA buffy]` en Coloquio. NO edites el README ni la rama.
Si te atascas: para y publica el error literal.
