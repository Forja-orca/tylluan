# 🔍 Workflow: Investigación Profunda
Guild: `scholars`  
Version: 1.0.0

---

## Cuándo Usar Este Workflow

Cuando se necesita **entender algo a fondo** antes de tomar una decisión técnica o responder una pregunta compleja. Evita el error de sintetizar sin haber leído las fuentes.

---

## Fases del Workflow

### Fase 0: Definir la Pregunta de Investigación (2 min)
```
Actor: researcher o analyst

Formular la pregunta con precisión:
  ❌ "Cómo funcionan los embeddings"
  ✅ "¿Cuál es el modelo de embedding más eficiente para búsqueda semántica 
      multilingual con menos de 500MB de RAM?"

Definir criterios de éxito:
  "La investigación está completa cuando puedo responder [pregunta específica]
   con al menos 3 fuentes verificadas."
```

---

### Fase 1: Búsqueda Inicial (10-20 min)
```
Actor: researcher (usa plugins: search, browser, pdf)

1. Búsqueda web con términos específicos
   → search("embedding models multilingual RAM comparison 2025")
   
2. Lectura de fuentes primarias (no resúmenes)
   → browser.navigate("url_del_artículo")
   
3. Si hay documentos PDF relevantes
   → pdf.extract("ruta/al/documento.pdf")

4. Guardar hallazgos intermedios:
   tylluan_remember("scholars:research — [pregunta]: [hallazgo X de fuente Y]")
```

**Output**: Lista de fuentes + hallazgos crudos

---

### Fase 2: Análisis y Síntesis (10-20 min)
```
Actor: analyst (usa plugins: sequential_thinking, data_tools)

1. Activar razonamiento estructurado si hay múltiples opciones:
   → sequential_thinking("compara las opciones A, B, C con criterios [lista]")

2. Identificar contradicciones entre fuentes
3. Filtrar por criterios de soberanía (local, sin APIs externas)
4. Construir tabla de comparación si aplica
```

**Output**: Síntesis estructurada con tabla comparativa

---

### Fase 3: Verificación
```
Actor: researcher

1. Para cada conclusión principal: identificar la fuente primaria
2. Si hay claims sin fuente → marcar como "hipótesis a validar"
3. Si hay gaps de información → documentarlos explícitamente
```

**Output**: Conclusiones verificadas + gaps identificados

---

### Fase 4: Persistir el Conocimiento
```
Actor: researcher o analyst

1. Guardar la síntesis en SilvaDB:
   tylluan_remember("scholars:finding — [pregunta]: [conclusión principal]")
   tylluan_remember("scholars:source — [URL/referencia]: [resumen de 1-2 frases]")

2. Si el hallazgo es relevante para el gremio builders:
   → Escribir en el Blackboard:
   "Investigación completada: [tema]. Disponible en SilvaDB como scholars:finding"

3. Si el hallazgo cambia una decisión arquitectural:
   → Notificar al architect del gremio builders
```

**Output**: Conocimiento persistido y compartido

---

## Herramientas del Workflow

| Fase | Plugin Primario | Plugin Secundario |
|------|----------------|------------------|
| Búsqueda | `search` | `browser` |
| Lectura | `browser`, `pdf` | `filesystem` |
| Análisis | `sequential_thinking` | `data_tools` |
| Extracción NER | `knowledge` | — |

---

## Anti-Patrones de Investigación

| ❌ Anti-patrón | ✅ Alternativa |
|--------------|--------------|
| Sintetizar desde el primer resultado de búsqueda | Leer al menos 3 fuentes independientes |
| Confundir un blog post con documentación oficial | Verificar siempre la fuente primaria |
| Buscar hasta confirmar lo que ya creías | Buscar activamente contra-argumentos |
| No guardar los hallazgos en SilvaDB | Siempre persistir con `tylluan_remember` |
| Presentar hipótesis como hechos | Distinguir claramente: "dato verificado" vs "hipótesis" |
