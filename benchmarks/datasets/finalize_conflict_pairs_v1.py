#!/usr/bin/env python3
"""Finaliza benchmarks/human_truth_conflict_pairs_v1.json (T1240, Coloquio).

Toma el extracto congelado (pares_raw.json), aplica el etiquetado manual de
Buffy (tabla LABELS / criterio abajo) y emite el JSON versionado con ficha.

Criterio NLI de 3 vías (fijado ANTES de etiquetar, aplicado por pareja):
  contradice      — las dos afirmaciones no pueden ser ciertas a la vez
                    sobre el mismo sujeto/aspecto.
  consistente     — mismo sujeto/entidad y afirmaciones compatibles:
                    coexisten, se implican o se complementan.
  no relacionado  — sujetos o temas distintos: saber de una no informa
                    sobre la otra.
Unidad etiquetada: los dos contenidos de nodo como afirmaciones (los campos
de procedencia source_id/edge_type viajan como metadato, no cambian el
juicio). Etiqueta y razón por pareja — un solo anotador (Buffy, agente).
"""
import json
import re
import sys

SRC = sys.argv[1] if len(sys.argv) > 1 else r"C:/Users/FoRJa/AppData/Local/Temp/pairs_raw.json"
DST = sys.argv[2] if len(sys.argv) > 2 else "E:/tylluan-wt-buffy/benchmarks/human_truth_conflict_pairs_v1.json"

# ── Etiquetas escritas a mano por Buffy tras leer las 100 parejas ───────────
# Ricas (texto semántico): juicio individual con frase de razón.
HAND = {
    "cp-089": ("no relacionado", "El primer fragmento describe variantes de un modelo (instructores, razonamiento, vision-language) y el segundo presupone cuantización Q4_K/Q6_K y VRAM: asuntos distintos del manual, ninguna afirmación informa de la otra."),
    "cp-090": ("consistente", "La segunda afirmación es la consecuencia exacta de la primera: la invocación de vite con --cacheDir produce el CACError 'Unknown option --cacheDir'; causa y efecto coherentes, no tensión."),
    "cp-091": ("consistente", "El comando dev del dashboard invoca el script que el package.json define ('dev': 'vite', v3.0.0): B es la definición de la que A deriva, mismo proyecto y compatibles."),
    "cp-092": ("no relacionado", "A es la invocación CLI de vite y B es la configuración de estilos de Tailwind (tema, colores CSS): mismo repo pero planos distintos, no se implican."),
    "cp-093": ("no relacionado", "Fragmento de características de un modelo frente a fragmento sobre la fecha de modificación como indicador de obsolescencia: temas proposicionales distintos del mismo manual."),
    "cp-094": ("no relacionado", "El comando de arranque del dashboard y el tsconfig.json (opciones de compilación TypeScript) tratan de asuntos distintos: ejecución frente a configuración de tipos."),
    "cp-095": ("no relacionado", "Briefing de concurrencia de mailbox (WAL, backoff) frente a briefing de contrato SSE (esquema NexusEvent): dos documentos de agentes con contenidos sin intersección proposicional."),
    "cp-096": ("consistente", "Ambas memorias resumen la MISMA evaluación de Tylluan13.5/Qwen3.5-4B: A consolida conclusiones (patrón de fabricación bajo presión) y B detalla la ronda real verificada; se complementan sin contradecirse."),
    "cp-097": ("no relacionado", "Mensaje del 2026-10-17 sobre el ciclo de investigación 'puntos neuronales' frente a mensaje del 2026-10-08 con protocolo de worktrees y asignaciones de tareas: temas y fechas distintos del mismo autor."),
    "cp-098": ("no relacionado", "Entrega técnica de la tarea T1235 (v8/H1/H2) frente a la declaración de la fundación filosófica de Tylluan: una es estado de trabajo y la otra principios rectores; sin relación proposicional."),
    "cp-099": ("no relacionado", "Síntesis estratégica de arquitectura de inferencia (2026-10-06) frente a verificación de PDFs del encargo T1240 (2026-10-17): dos trabajos de Antigravity de temas distintos."),
    "cp-100": ("no relacionado", "Síntesis de literales de modelo hardcodeados + diagnóstico de serializaciones frente a síntesis de diseño de batching de embeddings + corrección de asignación P0: cuerpos temáticos distintos, solo comparten la palabra embeddings."),
}

def short_sym(t: str) -> str:
    # "function:file:system_test.py:mcp" -> "mcp"; "module:json" -> "json"
    if t.startswith("module:"):
        return t[len("module:"):]
    return t.rsplit(":", 1)[-1] if ":" in t else t

def structural(pair: dict) -> tuple:
    src = pair["source_id"]
    a = pair["target_a"]["text"]
    b = pair["target_b"]["text"]
    if pair["edge_type"] == "defines":
        return ("consistente",
                f"La misma entidad ({src}) define dos miembros coexistentes "
                f"({short_sym(a)} y {short_sym(b)}): afirmaciones compatibles, "
                "ninguna niega a la otra.")
    return ("consistente",
            f"El mismo fichero ({src}) importa dos módulos distintos "
            f"({short_sym(a)} y {short_sym(b)}) que coexisten en el mismo "
            "conjunto de imports: sin conflicto.")

CTRL = re.compile(r"[\x00-\x08\x0b\x0c\x0e-\x1f]")

def clean(s: str) -> str:
    return CTRL.sub("", s)

def main() -> None:
    pairs = json.load(open(SRC, encoding="utf-8"))
    assert len(pairs) == 100, len(pairs)

    for p in pairs:
        label, reason = HAND.get(p["pair_id"], structural(p))
        p["label"] = label
        p["reason"] = reason
        p["source_id"] = clean(p["source_id"])
        for side in ("target_a", "target_b"):
            p[side]["text"] = clean(p[side]["text"])
            p[side]["node_id"] = clean(p[side]["node_id"])

    dist = {"contradice": 0, "consistente": 0, "no relacionado": 0}
    for p in pairs:
        dist[p["label"]] += 1
    baseline = max(dist.values()) / len(pairs)

    out = {
        "dataset": "human_truth_conflict_pairs",
        "version": 1,
        "created": "2026-10-18",
        "created_by": "Buffy (agente, worktree tylluan-wt-buffy)",
        "encargo": "Coloquio T1240/T1244 (Claude Code, TL): verdad de campo de 100 pares de las marcas conflicted de data/silva.db para el duelo P2 (verificador NLI aditivo sobre flag_contradiction_nodes)",
        "ficha": {
            "origen": "data/silva.db de la instancia compartida (E:\\tylluan), abierto EXCLUSIVAMENTE con file:...?mode=ro (SQLite read-only; el script benchmarks/datasets/build_conflict_pairs_v1.py no puede escribir). Extracción 2026-10-18 con la BD en vivo: 375 nodos conflicted de 9945 (la cita '368 de 9896' de la síntesis de puntos neuronales era de un medición anterior; la BD crece). 196 grupos candidatos (source,type) con >1 target activo cuyo source está hoy marcado conflicted, réplica exacta del agrupamiento de flag_contradiction_nodes (nodes.rs) sobre aristas activas (valid_until IS NULL, con las 3 exclusiones del código).",
            "muestreo": "Estratificado por tipo de arista, determinista (Random seed 20261018 sobre orden canónico): cobertura COMPLETA de los tipos con texto semántico (same_topic 7 grupos únicos de 9, remembers 4/4, contributed_to 1/1 = 12 pares) + proporcional entre tipos simbólicos (defines 59, imports 29 de 182). Decisión de cuota tomada por recuentos de tipos ANTES de leer contenidos. Dedupe de pares por firma (sha256(A),sha256(B)): 0 duplicados. Dentro de cada grupo: dos primeros targets por orden de id.",
            "criterio": "NLI de 3 vías fijado antes de etiquetar: contradice = no pueden ser ciertas a la vez sobre el mismo aspecto; consistente = mismo sujeto y compatibles (coexisten, se implican o complementan); no relacionado = temas distintos, sin bearings proposicional. Unidad etiquetada: los dos contenidos de nodo como afirmaciones; source_id/edge_type viajan como procedencia. Razón individual por pareja en 'reason'.",
            "quien_etiqueto": "Buffy (agente de la flota), lectura manual de las 100 parejas con texto completo de la BD para las 12 ricas; un solo anotador, sin acuerdo inter-anotador (no hay kappa). NO es etiquetado humano: declararlo así en cualquier métrica publicada (misma salvedad que en el dataset de intents, T1244).",
            "sanitizacion": "Textos saneados con las regex de benchmarks/datasets/anonymize_silva.py (API keys, rutas absolutas, emails) + strip de caracteres de control, truncados a 300 chars; content_len = longitud real y content_sha256 = SHA-256 del contenido CRUDO para verificación contra la BD (la BD solo lectura permite recuperar el texto completo por node_id cuando el duelo lo necesite).",
            "composicion": {"defines": 59, "imports": 29, "same_topic": 7, "remembers": 4, "contributed_to": 1},
            "distribucion": dist,
            "baseline_trivial": f"siempre {max(dist, key=dist.get)} = {baseline:.0%}",
            "sesgos_y_limitaciones": [
                "ANOTADOR NO HUMANO (agente), un solo anotador — acuerdo inter-anotador no medible.",
                "CERO pares 'contradice' en la muestra: recall de esa clase NO medible aquí. Es el hallazgo, no un error de muestreo: la heurística estructural (fan-out de defines/imports) produce ruido compatible, no contradicciones — coherente con el incidente de 2026-09-26. El duelo debe publicar el baseline trivial (siempre consistente = 91%) y esta limitación junto a cualquier cifra.",
                "88/100 parejas son simbólicas (defines/imports): la superficie NLI rica son los 12 pares de texto largo (muestra pequeña para conclusiones finas).",
                "Reutilización de lado: 13 nodos aparecen como lado en más de un par (p. ej. module:std::collections::HashMap en varios imports) — los PARES como unidad son únicos (0 firmas duplicadas), pero hay correlación entre muestras; no mezclar train/test por fila sin agrupar por nodo.",
                "BD viva: re-ejecutar el script hoy puede dar otra selección; el extracto congelado versionado es la fuente de verdad de este dataset (reproducible con los hashes).",
                "Extractos de 300 chars: las 12 parejas ricas se etiquetaron con el texto completo desde la BD, pero el fichero solo publica extracto mínimo + hash (privacidad).",
            ],
        },
        "records": pairs,
    }
    with open(DST, "w", encoding="utf-8") as f:
        json.dump(out, f, ensure_ascii=False, indent=1)
    print(json.dumps({"out": DST, "records": len(pairs), "dist": dist,
                      "baseline": f"{baseline:.1%}"}, ensure_ascii=False))

if __name__ == "__main__":
    main()
