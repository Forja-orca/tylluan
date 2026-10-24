# IMPL_H-25 — Cableado de check_readme_claims.sh en verify.sh (modo informe)

Entrega H-25 (T1667). Árbol base: `7b24da2` (origin/main real). Sin commits: dos ficheros modificados, diff abajo. Reset previo cumplido (copias guardadas en `E:\tylluan-patches\hermes\_saved\`, HEAD ya == origin/main; mis `??` H-24 verificados byte-iguales a la canónica `ca1162f` antes de seguir).

## Diff (git diff --stat: 2 ficheros, +16/-1)

1. **`scripts/verify.sh`** (+15, sección `--docs`, justo tras el gate de enlaces markdown y antes de Pilar 2): bloque report-only con comentario que ancla el porqué (spec «modo informe»; el live-count sigue en `check_test_count.sh` CI; este gate cazó la clase que check_test_count no ve — nunca lee README.es.md) y la advertencia explícita de no enrutar por `fail()` citando el incidente de 2026-09-13 que `test_verify_semantics.sh` existe para impedir.
2. **`scripts/test_verify_semantics.sh`** (+1/-1): `check_readme_claims` añadido a `REPORT_ONLY_GATES` — la declaración única que el meta-test ya consume.

```bash
    if bash scripts/check_readme_claims.sh; then
        ok "README claims vs tree consistency (check_readme_claims.sh, report-only)"
    else
        echo "⚠️  README claims drifted — see scripts/check_readme_claims.sh output above. Report-only, does not block this push."
    fi
```

## Pruebas del cableado (salidas reales de esta sesión)

- `bash -n scripts/verify.sh` → SYNTAX OK.
- **Meta-test de semántica** (`bash scripts/test_verify_semantics.sh`): T1 pasa; **T2 ejercita ahora explícitamente el gate nuevo** — línea real capturada: `-- T2: report-only gate 'check_readme_claims' fails (exit 1) -> --docs STILL exits 0` ✅; T3/T4 (negativo: re-cablearlo por `fail()` sí voltea el exit — el criterio es load-bearing); T6 (cero huérfanos: mi script queda oficialmente cableado); T7 pasa. **T5 falla VACUO en este host con y sin mi cambio** (verificado por stash/pristino: mismo fallo, `cp: cannot create .../wt5/STATUS.md` — el sandbox de T5 crea un worktree desechable con rutas /tmp que el git nativo mingw no resuelve: misma clase de bug MSYS que `agent_wt.sh`). En CI Linux T5 no debería verse afectado — No verificado aquí.
- **Simulación del bloque exacto** en árbol real con deriva presente (README.es 1125 en `7b24da2`, la fix del TL está en `ca1162f` sin subir): imprime `⚠️ README claims drifted … Report-only, does not block this push.` y el **exit del bloque es 0** — no bloquea.

## ## Impact

- Zona `scripts/` (gate ADR-013 aplica en revisión): no-depredación respondida — cero writes fuera de los dos ficheros declarados, cero edición de READMEs (excluido por spec), cero cambios en gates ajenos, cero tocar kernels/procesos. Semántica: puramente aditiva (un gate report-only + su clasificación).
- Riesgo asumido y querido: si el día de mañana la cifra del README.es vuelve a divergir, `verify.sh --docs` lo imprime sin bloquear el push (elegido por spec; endurecerlo a blocking sería decisión aparte del TL/José).
- Cobertura futura: T6 del meta-test impedirá que el script se orfanee; T2 impedirá que alguien lo convierta en blocking sin romper el test.

## No verificado

- `verify.sh --docs` completo NO corrí en este host: se cuelga en secciones de red/IO (supply-chain/npm audit y gates contra el kernel de producción vivo, aún en `955179f` esperando el reinicio de José; cargas pesadas prohibidas hasta el aviso del TL). La evidencia del cableado es el meta-test hermético + la simulación del bloque.
- T5 del meta-test en CI Linux: no verificable desde Windows (limitación local del sandbox de worktrees, pre-existente, reportada aquí para el TL).
- `check_no_predation.sh` no corrí (no hay commit; lo hará el TL al integrar).
