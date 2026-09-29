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
  - El triaje estático de los 29 "unknown" de la v3 (BENCHMARK_I7_J13.md sección
    0, comiteado en bca9238) se RECONSTRUYE desde el artefacto v3 y se contrasta
    contra sus conteos de referencia (17 args / 11 subtools / 1 hijack / 1 genuine).
  - Reintentos con backoff ante RATE_LIMITED/START_FAILED: son infra, no señal.

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
V3_RAW_FILE = ROOT / "benchmarks" / "benchmark_i7_j13_raw_calls.json"
RESULTS_JSON = ROOT / "benchmarks" / "benchmark_md3_results.json"

KERNEL_URL = os.environ.get("KERNEL_BASE", "http://127.0.0.1:47007")

# ── Prefijos MD-3 (fuente única de verdad: handler_do::error_prefixes) ─────────
ERR_WRAPPER = "❌ Error: "
AXIS_BY_PREFIX = {
    "ROUTING_FAILED:": ("routing_failed", None),
    "NO_GUILD_MATCH:": ("routing_failed", None),
    "RATE_LIMITED:":   ("infra_rate_limited", "resolved_via_error"),
    "START_FAILED:":   ("infra_start_failed", "resolved_via_error"),
    "NO_TOOLS:":       ("no_tools", "resolved_via_error"),
    "MISSING_ARGS:":   ("args_missing", "resolved_via_error"),
}
# Orden de match (los mensajes empiezan por el prefijo tras el wrapper).
PREFIX_ORDER = list(AXIS_BY_PREFIX.keys())

# Triaje estático de la v3 — conteos de referencia (BENCHMARK_I7_J13.md §0).
V3_REFERENCE = {
    "args_missing": 17,
    "sovereign_subtool": 11,
    "coordinator_hijack": 1,
    "genuine_no_match": 1,
}

# Intents del held-out que son nombres/subllamadas de sovereign tools del kernel
# (no guilds): dispatch legítimo de subtools, ítems del dataset que no miden
# routing. Detectados por patrón sobre el texto del intent.
SUBTOOL_INTENT_RE = re.compile(
    r"^\s*(tylluan_(do|remember|recall|think|graph)\b|query_model\b|doctor_diagnose\b|docker_status\b|explore\b)",
    re.IGNORECASE,
)
HIJACK_INTENT = "create a new branch called feature/vector-tiering"


def strip_error_wrapper(text: str) -> str:
    return text[len(ERR_WRAPPER):] if text.startswith(ERR_WRAPPER) else text


def api_call(url, data_dict, timeout=90, retries=2):
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

    Devuelve (eje, guild_extraido|None). El wrapper "❌ Error: " de
    registry/proxy::error_result se quita antes de matchear el prefijo.
    """
    body = strip_error_wrapper(error_text.strip())
    for prefix in PREFIX_ORDER:
        if body.startswith(prefix):
            axis, guild_flag = AXIS_BY_PREFIX[prefix]
            m = re.search(r"guild '([^']+)'", body)
            guild = m.group(1) if m else None
            if guild_flag is None:
                return axis, None
            return axis, guild
    return ("unprefixed_error", None)


def static_triage_from_v3(dataset_items):
    """Reconstruye el triaje estático de la v3 sobre los held-out.

    Clasifica los ítems que la v3 marcó como pred_live_matcher='unknown' usando
    las reglas de la sección 0 de BENCHMARK_I7_J13.md (hijack exacto → subtool
    soberano → error de args → genuine no-match) y contrasta contra los conteos
    de referencia. Devuelve (triage_por_id, conteos, desviaciones).
    """
    v3_unknown_ids = set()
    v3_response_by_id = {}
    if V3_RAW_FILE.exists():
        v3 = json.loads(V3_RAW_FILE.read_text(encoding="utf-8"))
        for rec in v3.get("detailed_items", []):
            if rec.get("pred_live_matcher") == "unknown":
                v3_unknown_ids.add(rec["id"])
        for call in v3.get("raw_calls", []):
            pass  # raw_calls solo trae muestras; el texto completo está en items
    # Texto de respuesta v3 por item (para regla de args): buscar en los items
    v3_resp = {}
    if V3_RAW_FILE.exists():
        v3 = json.loads(V3_RAW_FILE.read_text(encoding="utf-8"))
        for rec in v3.get("detailed_items", []):
            v3_resp[rec["id"]] = rec

    triage = {}
    by_target = {d["id"]: d for d in dataset_items}
    for item_id in sorted(v3_unknown_ids):
        d = by_target.get(item_id)
        if d is None:
            continue
        intent = d["intent"]
        resp_text = json.dumps(v3_resp.get(item_id, {}), ensure_ascii=False)
        if intent.strip() == HIJACK_INTENT:
            triage[item_id] = "coordinator_hijack"
        elif SUBTOOL_INTENT_RE.match(intent):
            triage[item_id] = "sovereign_subtool"
        elif "requires argument(s)" in resp_text:
            triage[item_id] = "args_missing"
        else:
            triage[item_id] = "genuine_no_match"
    counts = Counter(triage.values())
    deviations = {
        k: {"reference": v, "reconstructed": counts.get(k, 0)}
        for k, v in V3_REFERENCE.items()
        if counts.get(k, 0) != v
    }
    return triage, dict(counts), deviations, len(v3_unknown_ids)


def do_plan_call(intent, agent_id="md3-benchmark"):
    """POST /api/v1/do con plan=true. Devuelve dict normalizado del resultado."""
    res = api_call(f"{KERNEL_URL}/api/v1/do",
                   {"intent": intent, "plan": True, "agent_id": agent_id})
    out = {
        "status": res.get("status"),
        "is_error": bool(res.get("is_error", False)),
        "result": res.get("result"),
        "content_first": (res.get("content") or [""])[0] if res.get("content") else "",
        "raw_keys": sorted(res.keys()),
    }
    return out


def extract_plan_guild(result_obj):
    """Extrae guild de un result de plan-mode (o None)."""
    if isinstance(result_obj, dict):
        g = result_obj.get("guild")
        if isinstance(g, str) and g:
            return g
        # A veces el plan va anidado bajo "plan"
        plan = result_obj.get("plan")
        if isinstance(plan, dict):
            g = plan.get("guild")
            if isinstance(g, str) and g:
                return g
    return None


def main():
    print("=" * 78)
    print("MD-3 · ACCURACY DESCOMPUESTA (routing / args / validez de ítem)")
    print(f"Kernel objetivo: {KERNEL_URL}  (PROHIBIDO :47004 — kernel de test aislado)")
    print("=" * 78)

    # 1. Health + identidad del kernel (MD-2: sello de identidad obligatorio)
    try:
        health = kernel_health()
    except Exception as e:  # noqa: BLE001
        print(f"❌ FATAL: kernel de test no alcanzable en {KERNEL_URL}: {e}")
        print("   Arranca el kernel de test aislado (puerto 47007) y reintenta.")
        sys.exit(1)
    k_commit = health.get("commit", "unknown")
    k_version = health.get("version", "unknown")
    print(f"✅ Kernel de test: status={health.get('status')} version={k_version} commit={k_commit}")

    try:
        git_head = subprocess.check_output(
            ["git", "log", "-1", "--format=%h"], text=True, cwd=str(ROOT)).strip()
        lag = subprocess.check_output(
            ["git", "rev-list", "--count", f"{k_commit}..HEAD"], text=True, cwd=str(ROOT)).strip()
    except Exception:  # noqa: BLE001
        git_head, lag = "unknown", "unknown"
    print(f"📌 git HEAD={git_head} | lag kernel-vs-HEAD={lag} commits")

    # 2. Dataset
    ds = json.loads(DATASET_FILE.read_text(encoding="utf-8"))
    held_out = [d for d in ds["items"] if d["split"] == "held_out"]
    print(f"📚 Dataset: {len(held_out)} ítems held-out (de {ds.get('total_count')} totales)")

    # 3. Triaje estático (reconstrucción desde artefacto v3)
    triage, triage_counts, deviations, n_v3_unknown = static_triage_from_v3(held_out)
    print(f"\n--- TRIAJE ESTÁTICO v3 (reconstruido sobre {n_v3_unknown} unknown) ---")
    for k in ("args_missing", "sovereign_subtool", "coordinator_hijack", "genuine_no_match"):
        ref = V3_REFERENCE[k]
        got = triage_counts.get(k, 0)
        mark = "✓" if got == ref else "≠"
        print(f"  {k:20s} reconstruido={got:3d} referencia={ref:3d} {mark}")
    if deviations:
        print(f"  ⚠️ DESVIACIONES vs referencia: {json.dumps(deviations, ensure_ascii=False)}")
        print("     (reconciliar a mano antes de cerrar MD-3; se reportan igual)")

    # 4. Corrida real contra el kernel de test
    print(f"\n--- CORRIDA REAL: /api/v1/do plan=true × {len(held_out)} ítems ---")
    items_out = []
    for idx, d in enumerate(held_out):
        intent = d["intent"]
        target = d["target_guild"]
        rec = {
            "id": d["id"],
            "ambiguity_type": d["ambiguity_type"],
            "target_guild": target,
            "intent": intent,
            "static_validity": triage.get(d["id"], "routing_case"),
        }
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
                rec["outcome"] = "fractal_candidates"
                rec["error_prefix"] = None
                rec["extracted_guild"] = None
                rec["candidates"] = cands
                rec["routing_correct"] = target in cands
                rec["args_complete"] = None
            elif res["is_error"]:
                axis, guild = classify_dynamic(text)
                rec["outcome"] = axis
                rec["error_prefix"] = strip_error_wrapper(text.strip()).split(" ", 1)[0]
                rec["error_text"] = text[:400]
                rec["extracted_guild"] = guild
                if guild is not None:
                    # Guild resuelto y visible en el error (args/limit/start/no_tools):
                    # la DECISIÓN de guild es evaluable aunque no haya plan.
                    rec["routing_correct"] = (guild == target)
                else:
                    rec["routing_correct"] = False
                rec["args_complete"] = (axis not in ("args_missing",))
            else:
                guild = extract_plan_guild(res["result"])
                rec["outcome"] = "plan_ok"
                rec["error_prefix"] = None
                rec["extracted_guild"] = guild
                rec["routing_correct"] = (guild == target) if guild else None
                rec["args_complete"] = True if guild else None
                if guild is None:
                    rec["note"] = "plan ok sin campo guild extraíble"
        except Exception as e:  # noqa: BLE001
            rec["outcome"] = "http_error"
            rec["error_prefix"] = None
            rec["error_text"] = f"{type(e).__name__}: {e}"[:400]
            rec["extracted_guild"] = None
            rec["routing_correct"] = None
            rec["args_complete"] = None
        items_out.append(rec)
        if (idx + 1) % 10 == 0 or idx == len(held_out) - 1:
            print(f"  {idx + 1:3d}/{len(held_out)} procesados...")

    # Reintento para fallos de infra (rate-limit / start-failed): backoff corto
    infra = [r for r in items_out if r["outcome"] in ("infra_rate_limited", "infra_start_failed")]
    if infra:
        print(f"  ♻️ Reintentando {len(infra)} ítems con fallo de infra...")
        for rec in infra:
            time.sleep(2.0)
            try:
                res = do_plan_call(rec["intent"])
                text = res["content_first"] or ""
                if res["is_error"]:
                    axis, guild = classify_dynamic(text)
                    rec["outcome"] = axis
                    rec["error_prefix"] = strip_error_wrapper(text.strip()).split(" ", 1)[0]
                    rec["error_text"] = text[:400]
                    rec["extracted_guild"] = guild
                    rec["routing_correct"] = (guild == rec["target_guild"]) if guild else False
                    rec["args_complete"] = (axis not in ("args_missing",))
                elif res["status"] == "ambiguous":
                    raw = json.dumps(res, ensure_ascii=False)
                    cands = re.findall(r'"guild":\s*"([^"]+)"', raw)
                    rec["outcome"] = "fractal_candidates"
                    rec["candidates"] = cands
                    rec["routing_correct"] = rec["target_guild"] in cands
                else:
                    guild = extract_plan_guild(res["result"])
                    rec["outcome"] = "plan_ok"
                    rec["extracted_guild"] = guild
                    rec["routing_correct"] = (guild == rec["target_guild"]) if guild else None
                    rec["args_complete"] = True if guild else None
            except Exception as e:  # noqa: BLE001
                rec["error_text"] = f"retry_failed {type(e).__name__}: {e}"[:200]

    # 5. Las 4 cifras separadas
    n_total = len(items_out)
    routing_cases = [r for r in items_out if r["static_validity"] == "routing_case"]
    subtool_cases = [r for r in items_out if r["static_validity"] == "sovereign_subtool"]

    # Eje 1: accuracy de routing pura (excluye los ítems que no miden routing)
    evaluable = [r for r in routing_cases if r["routing_correct"] is not None]
    routing_correct = sum(1 for r in evaluable if r["routing_correct"])
    # Eje 2: completitud de args sobre guilds resueltos
    resolved = [r for r in routing_cases if r["extracted_guild"] or r["outcome"] == "plan_ok"]
    args_eval = [r for r in resolved if r["args_complete"] is not None]
    args_ok = sum(1 for r in args_eval if r["args_complete"])
    # Eje 3: validez de ítem (breakdown + cross-tab estático vs dinámico)
    validity_breakdown = dict(Counter(r["static_validity"] for r in items_out))
    outcome_counts = dict(Counter(r["outcome"] for r in items_out))
    cross_tab = {}
    for r in items_out:
        key = (r["static_validity"], "resolved" if r["outcome"] == "plan_ok" else r["outcome"])
        cross_tab[f"{key[0]}|{key[1]}"] = cross_tab.get(f"{key[0]}|{key[1]}", 0) + 1
    # Eje 4: end-to-end viejo (continuidad con bca9238)
    e2e_evaluable = [r for r in items_out if r["routing_correct"] is not None]
    e2e_correct = sum(1 for r in e2e_evaluable if r["routing_correct"])

    print("\n" + "=" * 78)
    print(f"RESULTADOS MD-3 (N={n_total}, kernel {k_commit}, HEAD {git_head})")
    print("=" * 78)
    n_rt = len(evaluable)
    print(f"(1) ROUTING puro (excluye no-routing-cases): {routing_correct}/{n_rt}"
          f" = {routing_correct / n_rt * 100:.2f}%" if n_rt else "(1) sin evaluable")
    n_ar = len(args_eval)
    print(f"(2) Completitud de args (guilds resueltos):  {args_ok}/{n_ar}"
          f" = {args_ok / n_ar * 100:.2f}%" if n_ar else "(2) sin resueltos")
    print(f"(3) Validez de ítem (triaje estático):        {json.dumps(validity_breakdown, ensure_ascii=False)}")
    print(f"    Subtools soberanos excluidos del eje 1:  {len(subtool_cases)}")
    print(f"(4) End-to-end viejo (comparabilidad v3):     {e2e_correct}/{len(e2e_evaluable)}"
          f" = {e2e_correct / len(e2e_evaluable) * 100:.2f}%" if e2e_evaluable else "(4) sin evaluable")
    print(f"    Outcomes dinámicos: {json.dumps(outcome_counts, ensure_ascii=False)}")

    # 6. Desglose por tipo de ambigüedad (solo routing cases)
    amb = {}
    for r in routing_cases:
        a = r["ambiguity_type"]
        s = amb.setdefault(a, {"total": 0, "correct": 0, "evaluable": 0})
        s["total"] += 1
        if r["routing_correct"] is not None:
            s["evaluable"] += 1
            s["correct"] += 1 if r["routing_correct"] else 0
    print("\nPor ambigüedad (routing cases):")
    for a, s in sorted(amb.items()):
        pct = s["correct"] / s["evaluable"] * 100 if s["evaluable"] else 0
        print(f"  {a:24s} N={s['total']:2d} correct={s['correct']:2d}/{s['evaluable']:2d} = {pct:.1f}%")

    payload = {
        "protocol": "MD-3 decomposed accuracy v1",
        "timestamp": time.strftime("%Y-%m-%d %H:%M:%S"),
        "kernel_test": {"url": KERNEL_URL, "commit": k_commit, "version": k_version},
        "git_head": git_head,
        "kernel_git_lag": lag,
        "static_triage": {
            "v3_unknown_count": n_v3_unknown,
            "reconstructed_counts": triage_counts,
            "reference_counts": V3_REFERENCE,
            "deviations": deviations,
            "per_item": triage,
        },
        "results": {
            "n_total": n_total,
            "routing_axis": {
                "n_routing_cases": len(routing_cases),
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
                "breakdown": validity_breakdown,
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
    print(f"\n💾 Artefacto escrito: {RESULTS_JSON}")
    return payload


if __name__ == "__main__":
    main()
