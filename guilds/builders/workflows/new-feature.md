# 📋 Workflow: Nueva Feature Completa
Guild: `builders`  
Version: 1.0.0

---

## Cuándo Usar Este Workflow

Cuando necesitas implementar una **feature nueva de principio a fin**: desde el diseño hasta los tests pasando por la implementación.

Usa este workflow como base. Los agentes del gremio pueden derivar workflows especializados a partir de él.

---

## Fases del Workflow

### Fase 0: Orientación (5 min)
```
Actor: architect o backend-dev

1. tylluan_recall("builders: features similares previas")
2. Leer ADRs relevantes en docs/architecture/ARCHITECTURE_DECISIONS.md
3. Definir el alcance mínimo viable (MVP de la feature)
4. Identificar qué archivos se tocarán (máx 5 para empezar)
```

**Output**: Lista de archivos a modificar + borrador de contrato de datos

---

### Fase 1: Diseño del Contrato (10 min)
```
Actor: architect

1. Definir inputs/outputs de la feature
   - Si es API: schema JSON de request/response
   - Si es guild: firma de tool MCP
   - Si es UI: qué datos muestra y qué acciones permite

2. Escribir ADR si es una decisión no trivial:
   ## Contexto: [por qué se necesita]
   ## Decisión: [qué se va a hacer]  
   ## Consecuencias: [qué cambia, qué riesgos hay]

3. tylluan_remember("builders:decision — [ADR resumido]")
```

**Output**: Contrato documentado + ADR (si aplica)

---

### Fase 2: Implementación Backend (variable)
```
Actor: backend-dev (+ rust-specialist si se necesita)

Regla: cambios de <20 líneas, verificar con cargo check entre cada cambio

Loop {
    1. Implementar el cambio mínimo siguiente
    2. cargo check -p tylluan-kernel (debe pasar, si no: fix antes de seguir)
    3. Si pasa → continuar
    4. Si hay tests afectados → cargo test -p tylluan-kernel
}

Al terminar backend:
    cargo test -p tylluan-kernel 2>&1 | tail -5
    → Debe mostrar: "test result: ok. N passed"
```

**Output**: Código backend funcionando con tests verdes

---

### Fase 3: Implementación Frontend (variable, si aplica)
```
Actor: frontend-dev

1. Agregar método al NexusBridge (nexus-bridge.ts)
2. Crear/actualizar el componente React
3. Conectar via useNexus() hook
4. npm run build → verificar que no hay errores TypeScript
```

**Output**: UI conectada al backend

---

### Fase 4: Integración y Validación
```
Actor: backend-dev o architect

1. Levantar el kernel: cargo run -p tylluan-kernel
2. Levantar el dashboard: npm run dev (en dashboard_v3/)
3. Probar el flujo completo manualmente
4. Verificar que los tests siguen en verde: cargo test
5. Verificar que no hay regresiones en features adyacentes
```

**Output**: Feature validada end-to-end

---

### Fase 5: Commit y Memoria
```
Actor: backend-dev o quien hizo el trabajo principal

1. git add (solo los archivos de la feature)
2. git commit -m "feat: [descripción en imperativo, <72 chars]"
3. tylluan_remember("builders:feature — [nombre]: [qué hace, qué patrones usa]")
4. Actualizar docs si la feature cambia un comportamiento observable
```

**Output**: Commit limpio + conocimiento persistido en SilvaDB

---

## Variantes de Este Workflow

Los agentes del gremio pueden crear variantes guardándolas en `sandbox/experiments/` y luego proponiéndolas como nuevos workflows:

- `new-feature-rust-only.md` — Para features solo en el kernel Rust
- `new-feature-guild.md` — Para crear un nuevo guild Python completo  
- `new-feature-api-only.md` — Para nuevos endpoints sin UI

---

## Anti-Patrones a Evitar

| ❌ Anti-patrón | ✅ Alternativa |
|--------------|--------------|
| Implementar todo de golpe y luego testear | Validar con cargo check después de cada cambio |
| Empezar por el código sin definir el contrato | Siempre Fase 1 antes de Fase 2 |
| Modificar >5 archivos simultáneamente | Dividir en sub-features más pequeñas |
| Asumir que "funciona" sin cargo test | Siempre ejecutar los tests antes del commit |
| Commitear sin mensaje descriptivo | `git commit -m "feat: [qué hace exactamente]"` |
