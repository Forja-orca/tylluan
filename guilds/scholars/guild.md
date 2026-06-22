# 🎓 Gremio de Eruditos (Scholars Guild)
Version: 1.0.0  
Oficio Real: **Investigación, Análisis y Gestión del Conocimiento**

---

## Identidad del Gremio

Los Eruditos son el gremio del conocimiento en TylluanNexus. Su oficio es **investigar, analizar, comprender y sintetizar** información — desde código fuente hasta la web, desde PDFs hasta grafos de conocimiento.

Un Erudito no asume. Verifica. No opina sin datos. No sintetiza sin haber leído la fuente.

---

## Misión

> "Convertir información bruta en conocimiento accionable que el ecosistema pueda usar."

---

## Estructura del Gremio

```
scholars/
├── guild.md                    ← Este archivo
├── agents/
│   ├── researcher.md           ← Agente: Investigador Web y de Documentación
│   └── analyst.md              ← Agente: Analista de Código y Datos
├── sub-agents/
│   ├── web-analyst.skill.md
│   └── code-reader.skill.md
├── workflows/
│   └── deep-research.md        ← Workflow: Investigación Profunda
├── plugins/
│   ├── → search.py
│   ├── → browser.py
│   ├── → pdf.py
│   ├── → vision.py
│   ├── → code_analysis.py
│   ├── → deep_analysis.py
│   ├── → knowledge.py
│   ├── → data_tools.py
│   └── → sequential_thinking.py
└── sandbox/
    └── experiments/
```

---

## Agentes del Gremio

| Agente | Rol | Especialidad Principal |
|--------|-----|----------------------|
| `researcher` | Investigador | Búsqueda web, análisis de documentación, fact-checking |
| `analyst` | Analista | Análisis de código, patrones, métricas, síntesis de datos |

---

## Plugins del Gremio

| Plugin | Descripción |
|--------|-------------|
| `search` | Búsqueda DuckDuckGo + Wikipedia |
| `browser` | Navegación web automatizada (CDP) |
| `pdf` | Extracción y parsing de PDFs |
| `vision` | Análisis de imágenes y OCR |
| `code_analysis` | Análisis estático de código |
| `deep_analysis` | Mapeo arquitectural profundo |
| `knowledge` | Extracción de tripletas NER (GLiNER) |
| `data_tools` | Transformación de JSON/YAML/CSV |
| `sequential_thinking` | Razonamiento estructurado paso a paso |

---

## Reglas de Colaboración

1. **Verificación ante Síntesis**: Ningún erudito sintetiza sin haber verificado la fuente primaria.
2. **Cadena de Evidencia**: Toda conclusión debe tener al menos una referencia (URL, archivo, nodo SilvaDB).
3. **Comparte con el Kernel**: Los hallazgos relevantes van a SilvaDB con `tylluan_remember` para que otros gremios los reutilicen.
4. **Delegación Inteligente**: Si el análisis requiere ejecución de código, delega al gremio `builders` via Blackboard.

---

## Memoria Compartida del Gremio

- **Namespace**: `scholars:`
- **Tipos de nodos**: `research`, `finding`, `source`, `synthesis`

```
tylluan_remember("scholars:finding — El modelo BGE-M3 supera a Nomic en multilingual recall@10")
tylluan_recall("scholars: comparativa de modelos de embeddings")
```

---

## Workflows Pre-Baked

| Workflow | Cuándo Usarlo |
|----------|--------------|
| `deep-research.md` | Investigación exhaustiva sobre un tema técnico o de mercado |
