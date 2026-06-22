# 🐛 Workflow: Sesión de Debugging Estructurado
Guild: `builders`  
Version: 1.0.0

---

## Cuándo Usar Este Workflow

Cuando hay un bug en producción o durante desarrollo y necesitas resolverlo de forma sistemática sin perder contexto.

---

## Fases del Workflow

### Fase 0: Triage (2 min)
```
Actor: guardian (wardens guild) o backend-dev

1. ¿El bug es reproducible? ¿En qué condiciones?
2. ¿Cuándo apareció? (último commit que funcionaba: git bisect)
3. ¿Qué muestra el log?
   tail -50 logs/kernel.log | grep ERROR
4. Clasificar: panic | compile error | logic error | performance | intermittent
```

**Output**: Clasificación del bug + contexto mínimo

---

### Fase 1: Aislamiento
```
Actor: backend-dev

1. Reproducir el bug con el test más pequeño posible
   cargo test nombre_del_test 2>&1

2. Grepar el error en el código fuente:
   grep -rn "mensaje_de_error" crates/

3. Leer SOLO el código relevante (±20 líneas del punto de fallo)

4. Formular hipótesis:
   "El bug ocurre porque [razón específica]"
```

**Output**: Hipótesis documentada + test que reproduce el bug

---

### Fase 2: Fix Mínimo
```
Actor: backend-dev (+ rust-specialist si es lifetime/unsafe)

Regla: El fix debe ser el más pequeño posible.
Si requiere >30 líneas, probablemente hay un problema de diseño → consultar architect.

1. Implementar el fix
2. cargo check (debe compilar)
3. Verificar que el test del bug ahora pasa
4. Verificar que los tests existentes no se rompieron:
   cargo test -p tylluan-kernel
```

**Output**: Fix implementado con test que valida la corrección

---

### Fase 3: Root Cause Analysis y Prevención
```
Actor: architect o backend-dev senior

1. ¿Por qué ocurrió el bug?
   - Falta de validación de input
   - Race condition
   - Asunción incorrecta sobre el estado
   - Falta de timeout

2. ¿Se puede repetir en otro lugar? Grepar patrones similares.

3. Documentar el aprendizaje:
   tylluan_remember("builders:lesson — [descripción del bug y fix]")
   tylluan_remember("wardens:incident — [timestamp]: [descripción]")
```

**Output**: Root cause documentado + prevención implementada (si aplica)

---

### Fase 4: Commit
```
1. git add (solo archivos del fix)
2. git commit -m "fix: [descripción del bug resuelto]"
```

---

## Herramientas Útiles para Debugging

```bash
# Ver últimos errores del kernel
tail -100 logs/kernel.log | grep -E "ERROR|WARN|panic"

# Compilar y ver errores completos
cargo check -p tylluan-kernel 2>&1

# Ejecutar un test específico con output detallado
cargo test test_nombre -- --nocapture

# Ver qué cambió recientemente
git log --oneline -10
git diff HEAD~1
```

---

## Escalado de Incidentes

| Severidad | Criterio | Acción |
|-----------|----------|--------|
| 🔴 Critical | Kernel no arranca / datos corruptos | Activar guardian (wardens) + architect inmediatamente |
| 🟠 High | Feature rota pero kernel funciona | backend-dev + debug session ahora |
| 🟡 Medium | Comportamiento inesperado no crítico | backend-dev + agregar al backlog |
| 🟢 Low | Cosmético / edge case raro | Documentar en SilvaDB + fix en próxima sesión |
