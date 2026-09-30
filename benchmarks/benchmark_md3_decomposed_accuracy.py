#!/usr/bin/env python3
"""
MD-3 — Protocolo de accuracy DESCOMPUESTA del router en vivo.

Cierra MD-3 de docs/architecture/MEASUREMENT_DEBT_REGISTER.md: la "accuracy"
end-to-end de tylluan_do mezcla (a) decisión de guild, (b) completitud de args,
(c) validez del ítem como caso de routing. Este harness separa los tres ejes
usando los prefijos de error estables de Stage-1 (handler_do::error_prefixes):

    ROUTING_FAILED:  guild no resuelto (hint desconocido, RFL, baja confianza)
    NO_GUILD_MATCH:  ningún guild pasó el umbral (preexistente)
    RATE_LIMITED:    rate limit per-guild (infra, no señal de routing)
    START_FAILED:    el guild no arrancó (infra)
    NO_TOOLS:        guild registrado sin tools
    MISSING_ARGS:    guild resuelto pero args obligatorios ausentes

Reglas de ejecución:
  - NUNCA apunta a producción (:47004). Default: kernel de test aislado :47007
    (env var KERNEL_BASE para override). Patrón test-run-p1 (MD-5/T4).
  - Una sola métrica real por intent: POST /api/v1/do {"intent":..., "plan":true}.
    plan=true NO ejecuta la tool; resuelve guild+tool+args (M31-P2).
  - Validez de ítem (eje intrínseco, 3-way): routing_case / sovereign_subtool /
    coordinator_hijack. El triaje manual v3 (BENCHMARK_I7_J13.md §0, bca9238:
    17 args / 11 subtools / 1 hijack / 1 genuine) NO es reconstruible per-item
    desde los artefactos comiteados (los textos de error exactos no se
    guardaron por ítem); su desglose args-vs-genuine lo reemplaza, a partir de
    ahora, la clasificación máquina-legible por prefijos que este protocolo
    produce en vivo. Los conteos v3 se citan como referencia histórica.
  - Reintentos con backoff ante RATE_LIMITED/START_FAILED: son infra, no señal.
  - Progreso incremental: cada ítem se apendea a benchmark_md3_items.jsonl
    (una corrida cortada por timeout nunca pierde los ítems ya medidos).

Salida: benchmarks/benchmark_md3_results.json con las 4 cifras separadas.
"""

import json
import os
import re
import subprocess
import sys
import time
import urllib.request
import urllib.error
from collections import Counter
from pathlib import Path

if sys.platform == "win32":
    sys.stdout.reconfigure(encoding='utf-8', errors='replace')
    sys.stderr.reconfigure(encoding='utf-8', errors='replace')

ROOT = Path(__file__).resolve().parent.parent
DATASET_FILE = ROOT / "benchmarks" / "dataset_i7_routing_curated.json"
RESULTS_JSON = ROOT / "benchmarks" / "benchmark_md3_results.json"
ITEMS_JSONL = ROOT / "benchmarks" / "benchmark_md3_items.jsonl"

KERNEL_URL = os.environ.get("KERNEL_BASE", "http://127.0.0.1:47007")

# ── Prefijos MD-3 (fuente única de verdad: handler_do::error_prefixes) ─────────
ERR_WRAPPER = "❌ Error: "
AXIS_BY_PREFIX = {
    "ROUTING_FAILED:": ("routing_failed", False),
    "NO_GUILD_MATCH:": ("routing_failed", False),
    "RATE_LIMITED:":   ("infra_rate_limited", True),
    "START_FAILED:":   ("infra_start_failed", True),
    "NO_TOOLS:":       ("no_tools", False),
    "MISSING_ARGS:":   ("args_missing", False),
}
PREFIX_ORDER = list(AXIS_BY_PREFIX.keys())

# Referencia HISTÓRICA (triaje manual v3, BENCHMARK_I7_J13.md §0, bca9238).
# No se usa para clasificar: se cita para continuidad y se contrasta en el
# informe con lo que el protocolo nuevo mide en vivo.
V3_REFERENCE = {
    "args_missing": 17,
    "sovereign_subtool": 11,
    "coordinator_hijack": 1,
    "genuine_no_match": 1,
}

# Intents del held-out que son nombres/subllamadas de sovereign tools del kernel
# (no guilds): dispatch legítimo de subtools, ítems del dataset que no miden
# routing. Detectados por patrón sobre el texto del intent (eje intrínseco).
SUBTOOL_INTENT_RE = re.compile(
    r"^\s*(tylluan_(do|remember|recall|think|graph)\b|query_model\b|doctor_diagnose\b|docker_status\b|explore\b)",
    re.IGNORECASE,
)
HIJACK_INTENT = "create a new branch called feature/vector-tiering"


def strip_error_wrapper(text: str) -> str:
    return text[len(ERR_WRAPPER):] if text.startswith(ERR_WRAPPER) else text


def api_call(url, data_dict, timeout=120, retries=2):
    payload = json.dumps(data_dict).encode("utf-8")
    last_exc = None
    for attempt in range(retries + 1):
        try:
            req = urllib.request.Request(
                url, data=payload,
                headers={"Content-Type": "application/json"}, method="POST",
            )
            with urllib.request.urlopen(req, timeout=timeout) as resp:
                return json.loads(resp.read())
        except Exception as e:  # noqa: BLE001 — reintentar cualquier fallo de red
            last_exc = e
            if attempt < retries:
                time.sleep(1.5 * (attempt + 1))
    raise last_exc


def kernel_health():
    with urllib.request.urlopen(f"{KERNEL_URL}/health", timeout=10) as r:
        return json.loads(r.read())


def classify_dynamic(error_text: str):
    """Clasifica un texto de error de tylluan_do por prefijo estable MD-3.

    Devuelve (eje, es_infra, guild_extraido|None). El wrapper "❌ Error: " de
    registry/proxy::error_result se quita antes de matchear el prefijo.
    """
    body = strip_error_wrapper(error_text.strip())
    for prefix in PREFIX_ORDER:
        if body.startswith(prefix):
            axis, is_infra = AXIS_BY_PREFIX[prefix]
            m = re.search(r"guild '([^']+)'", body)
            guild = m.group(1) if m else None
            return axis, is_infra, guild
    return ("unprefixed_error", False, None)


def static_validity(item):
    """Eje intrínseco de validez del ítem (3-way, por naturaleza del ítem)."""
    intent = item["intent"].strip()
    if intent == HIJACK_INTENT:
        return "coordinator_hijack"
    if SUBTOOL_INTENT_RE.match(intent):
        return "sovereign_subtool"
    return "routing_case"


def do_plan_call(intent, agent_id="md3-benchmark"):
    """POST /api/v1/do con plan=true. Devuelve dict normalizado del resultado."""
    res = api_call(f"{KERNEL_URL}/api/v1/do",
                   {"intent": intent, "plan": True, "agent_id": agent_id})
    return {
        "status": res.get("status"),
        "is_error": bool(res.get("is_error", False)),
        "result": res.get("result"),
        "content_first": (res.get("content") or [""])[0] if res.get("content") else "",
        "raw_keys": sorted(res.keys()),
    }


def extract_plan_guild(result_obj):
    if isinstance(result_obj, dict):
        g = result_obj.get("guild")
        if isinstance(g, str) and g:
            return g
        plan = result_obj.get("plan")
        if isinstance(plan, dict):
            g = plan.get("guild")
            if isinstance(g, str) and g:
                return g
    return None


def measure_one(intent, target):
    """Una medición real. Devuelve el record del ítem (sin campos de dataset)."""
    rec = {}
    try:
        res = do_plan_call(intent)
        text = res["content_first"] or ""
        if res["status"] == "ambiguous":
            # Fractal gate (M23): candidatos sin resolver guild único.
            cands = []
            if isinstance(res["result"], dict):
                cands = [c.get("guild") for c in res["result"].get("candidates", [])
                         if isinstance(c, dict)]
            elif res["result"] is None:
                raw = json.dumps(res, ensure_ascii=False)
                cands = re.findall(r'"guild":\s*"([^"]+)"', raw)
            rec.update(outcome="fractal_candidates", error_prefix=None,
                       extracted_guild=None, candidates=cands,
                       routing_correct=target in cands, args_complete=None)
        elif res["is_error"]:
            axis, is_infra, guild = classify_dynamic(text)
            rec.update(outcome=axis,
                       error_prefix=strip_error_wrapper(text.strip()).split(" ", 1)[0],
                       error_text=text[:400], extracted_guild=guild, is_infra=is_infra)
            if is_infra:
                rec["routing_correct"] = None
                rec["args_complete"] = None
            else:
                rec["routing_correct"] = (guild == target) if guild is not None else False
                rec["args_complete"] = (axis != "args_missing")
        else:
            guild = extract_plan_guild(res["result"])
            rec.update(outcome="plan_ok", error_prefix=None, extracted_guild=guild,
                       is_infra=False, is_error=False)
            rec["routing_correct"] = (guild == target) if guild else None
            rec["args_complete"] = True if guild else None
            if guild is None:
                rec["note"] = "plan ok sin campo guild extraíble"
    except Exception as e:  # noqa: BLE001
        rec.update(outcome="http_error", error_prefix=None,
                   error_text=f"{type(e).__name__}: {e}"[:400],
                   extracted_guild=None, routing_correct=None, args_complete=None)
    return rec


def main():
    print("=" * 78, flush=True)
    print("MD-3 · ACCURACY DESCOMPUESTA (routing / args / validez de ítem)", flush=True)
    print(f"Kernel objetivo: {KERNEL_URL}  (PROHIBIDO :47004 — kernel de test aislado)", flush=True)
    print("=" * 78, flush=True)

    # 1. Health + identidad del kernel (MD-2: sello de identidad obligatorio)
    try:
        health = kernel_health()
    except Exception as e:  # noqa: BLE001
        print(f"❌ FATAL: kernel de test no alcanzable en {KERNEL_URL}: {e}")
        print("   Arranca el kernel de test aislado (puerto 47007) y reintenta.")
        sys.exit(1)
    k_commit = health.get("commit", "unknown")
    k_version = health.get("version", "unknown")
    print(f"✅ Kernel de test: status={health.get('status')} version={k_version} commit={k_commit}", flush=True)

    try:
        git_head = subprocess.check_output(
            ["git", "log", "-1", "--format=%h"], text=True, cwd=str(ROOT)).strip()
        lag = subprocess.check_output(
            ["git", "rev-list", "--count", f"{k_commit}..HEAD"], text=True, cwd=str(ROOT)).strip()
    except Exception:  # noqa: BLE001
        git_head, lag = "unknown", "unknown"
    print(f"📌 git HEAD={git_head} | lag kernel-vs-HEAD={lag} commits", flush=True)

    # 2. Dataset
    ds = json.loads(DATASET_FILE.read_text(encoding="utf-8"))
    held_out = [d for d in ds["items"] if d["split"] == "held_out"]
    print(f"📚 Dataset: {len(held_out)} ítems held-out (de {ds.get('total_count')} totales)", flush=True)

    # 3. Corrida real contra el kernel de test (progreso incremental en JSONL)
    print(f"\n--- CORRIDA REAL: /api/v1/do plan=true × {len(held_out)} ítems ---", flush=True)
    ITEMS_JSONL.write_text("", encoding="utf-8")  # truncar de corridas previas
    items_out = []
    with open(ITEMS_JSONL, "a", encoding="utf-8") as jsonl:
        for idx, d in enumerate(held_out):
            rec = {
                "id": d["id"],
                "ambiguity_type": d["ambiguity_type"],
                "target_guild": d["target_guild"],
                "intent": d["intent"],
                "static_validity": static_validity(d),
            }
            rec.update(measure_one(d["intent"], d["target_guild"]))
            items_out.append(rec)
            jsonl.write(json.dumps(rec, ensure_ascii=False) + "\n")
            jsonl.flush()
            if (idx + 1) % 5 == 0 or idx == len(held_out) - 1:
                print(f"  {idx + 1:3d}/{len(held_out)} procesados...", flush=True)

    # 4. Reintento único para fallos de infra (backoff corto; máx 2 rondas)
    for round_no in (1, 2):
        infra = [r for r in items_out if r.get("outcome") in ("infra_rate_limited", "infra_start_failed", "http_error")]
        if not infra:
            break
        print(f"  ♻️ Ronda {round_no}: reintentando {len(infra)} ítems con fallo de infra...", flush=True)
        with open(ITEMS_JSONL, "a", encoding="utf-8") as jsonl:
            for rec in infra:
                time.sleep(2.0)
                rec.update(measure_one(rec["intent"], rec["target_guild"]))
                rec["infra_retries"] = rec.get("infra_retries", 0) + 1
                jsonl.write(json.dumps(rec, ensure_ascii=False) + "\n")
                jsonl.flush()

    # 5. Las 4 cifras separadas
    n_total = len(items_out)
    routing_cases = [r for r in items_out if r["static_validity"] == "routing_case"]
    subtool_cases = [r for r in items_out if r["static_validity"] == "sovereign_subtool"]
    hijack_cases = [r for r in items_out if r["static_validity"] == "coordinator_hijack"]
    infra_outcomes = [r for r in items_out if r.get("outcome") in
                      ("infra_rate_limited", "infra_start_failed")]

    # Eje 1: accuracy de routing pura — solo routing cases con medición evaluable
    # (excluye no-routing-cases y fallos de infra; los infra se cuentan aparte)
    evaluable = [r for r in routing_cases if r["routing_correct"] is not None]
    routing_correct = sum(1 for r in evaluable if r["routing_correct"])
    # Eje 2: completitud de args sobre routing cases con guild resuelto visible
    resolved = [r for r in routing_cases if r["extracted_guild"] or r["outcome"] == "plan_ok"]
    args_eval = [r for r in resolved if r["args_complete"] is not None]
    args_ok = sum(1 for r in args_eval if r["args_complete"])
    # Eje 3: validez de ítem (breakdown intrínseco + cross-tab contra outcome vivo)
    validity_breakdown = dict(Counter(r["static_validity"] for r in items_out))
    outcome_counts = dict(Counter(r["outcome"] for r in items_out))
    cross_tab = {}
    for r in items_out:
        key = f"{r['static_validity']}|{r['outcome']}"
        cross_tab[key] = cross_tab.get(key, 0) + 1
    # Eje 4: end-to-end viejo (continuidad con bca9238: den = evaluable)
    e2e_evaluable = [r for r in items_out if r["routing_correct"] is not None]
    e2e_correct = sum(1 for r in e2e_evaluable if r["routing_correct"])

    print("\n" + "=" * 78, flush=True)
    print(f"RESULTADOS MD-3 (N={n_total}, kernel {k_commit}, HEAD {git_head})", flush=True)
    print("=" * 78, flush=True)
    n_rt = len(evaluable)
    print(f"(1) ROUTING puro (excluye no-routing-cases e infra): {routing_correct}/{n_rt}"
          + (f" = {routing_correct / n_rt * 100:.2f}%" if n_rt else " (sin evaluables)"))
    n_ar = len(args_eval)
    print(f"(2) Completitud de args (guilds resueltos):          {args_ok}/{n_ar}"
          + (f" = {args_ok / n_ar * 100:.2f}%" if n_ar else " (sin resueltos)"))
    print(f"(3) Validez de ítem (intrínseca):                     {json.dumps(validity_breakdown, ensure_ascii=False)}")
    print(f"    Excluidos del eje 1: subtools={len(subtool_cases)} hijack={len(hijack_cases)} infra={len(infra_outcomes)}")
    print(f"(4) End-to-end (denominador evaluable):               {e2e_correct}/{len(e2e_evaluable)}"
          + (f" = {e2e_correct / len(e2e_evaluable) * 100:.2f}%" if e2e_evaluable else " (sin evaluables)"))
    print(f"    Outcomes dinámicos: {json.dumps(outcome_counts, ensure_ascii=False)}", flush=True)
    print(f"    Referencia histórica v3 (triaje manual, N=29 unknown): {json.dumps(V3_REFERENCE, ensure_ascii=False)}", flush=True)

    # Desglose por tipo de ambigüedad (solo routing cases)
    amb = {}
    for r in routing_cases:
        a = r["ambiguity_type"]
        s = amb.setdefault(a, {"total": 0, "correct": 0, "evaluable": 0})
        s["total"] += 1
        if r["routing_correct"] is not None:
            s["evaluable"] += 1
            s["correct"] += 1 if r["routing_correct"] else 0
    print("\nPor ambigüedad (routing cases):", flush=True)
    for a, s in sorted(amb.items()):
        pct = s["correct"] / s["evaluable"] * 100 if s["evaluable"] else 0
        print(f"  {a:24s} N={s['total']:2d} correct={s['correct']:2d}/{s['evaluable']:2d} = {pct:.1f}%")

    payload = {
        "protocol": "MD-3 decomposed accuracy v1",
        "timestamp": time.strftime("%Y-%m-%d %H:%M:%S"),
        "kernel_test": {"url": KERNEL_URL, "commit": k_commit, "version": k_version},
        "git_head": git_head,
        "kernel_git_lag": lag,
        "v3_manual_triage_reference": V3_REFERENCE,
        "results": {
            "n_total": n_total,
            "routing_axis": {
                "n_routing_cases": len(routing_cases),
                "n_excluded_no_routing_cases": len(subtool_cases) + len(hijack_cases),
                "n_excluded_infra": len(infra_outcomes),
                "n_evaluable": n_rt,
                "correct": routing_correct,
                "accuracy": round(routing_correct / n_rt, 4) if n_rt else None,
            },
            "args_axis": {
                "n_guilds_resolved": len(resolved),
                "n_evaluable": n_ar,
                "args_complete": args_ok,
                "rate": round(args_ok / n_ar, 4) if n_ar else None,
            },
            "validity_axis": {
                "breakdown_intrinsic": validity_breakdown,
                "outcomes_dynamic": outcome_counts,
                "cross_tab_static_vs_dynamic": cross_tab,
            },
            "legacy_end_to_end": {
                "n": len(e2e_evaluable),
                "correct": e2e_correct,
                "accuracy": round(e2e_correct / len(e2e_evaluable), 4) if e2e_evaluable else None,
                "v3_reference": {"n": 77, "live_accuracy": 0.3636, "j13_hybrid": 0.6104},
            },
        },
        "per_ambiguity": amb,
        "items": items_out,
    }
    RESULTS_JSON.write_text(json.dumps(payload, indent=2, ensure_ascii=False), encoding="utf-8")
    print(f"\n💾 Artefactos: {RESULTS_JSON} + {ITEMS_JSONL}", flush=True)
    return payload


if __name__ == "__main__":
    main()
