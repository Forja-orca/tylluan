# 🎓 Gremio de Eruditos (Scholars Guild)
Version: 1.0.0  
Oficio Real: **Investigación, Análisis y Gestión del Conocimiento**

---

> **Nota (2026-10-14).** Este documento se corrigió contra el árbol real de `guilds/`. Lo que aquí figura como existente existe; lo que falta está marcado como "no existe". Los *agentes/roles* son ficheros de texto para cargar en un cliente IDE, no procesos del kernel; los procesos que el kernel arranca son los plugins Python.

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
├── (agents/ y sub-agents/: no existen; researcher y analyst son roles previstos)
├── workflows/
│   └── deep-research.md        ← Workflow: Investigación Profunda
└── plugins/                    ← ficheros reales: search, pdf, deep_analysis, knowledge, data_tools,
                                  sequential_thinking, memory, memory_bridge, ingest, ast_surgeon, silva_utils
                                  (browser y vision viven en guilds/core/; code_analysis en builders/plugins/;
                                   no hay sandbox/)
```

---

## Agentes del Gremio

| Agente | Rol | Especialidad Principal |
|--------|-----|----------------------|
| `researcher` | Investigador (rol previsto, sin fichero) | Búsqueda web, análisis de documentación, fact-checking |
| `analyst` | Analista (rol previsto, sin fichero) | Análisis de código, patrones, métricas, síntesis de datos |

---

## Plugins del Gremio

| Plugin | Descripción |
|--------|-------------|
| `search` | Búsqueda DuckDuckGo + Wikipedia |
| `browser` | Navegación web automatizada (CDP) — en `guilds/core/browser.py` |
| `pdf` | Extracción y parsing de PDFs |
| `vision` | Análisis de imágenes y OCR — en `guilds/core/vision.py` |
| `code_analysis` | Análisis estático de código — en `guilds/builders/plugins/` |
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
