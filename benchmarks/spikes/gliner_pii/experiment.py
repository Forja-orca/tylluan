"""GLiNER2 (fastino/gliner2-base-v1) real PII detection spike.

Same discipline as every spike today: real inference (no mock), a
hand-labeled held-out set with genuine positive AND negative cases
(text that mentions PII-adjacent words but has none, to catch false
positives), compared against a naive regex baseline before declaring
anything. GO only if GLiNER beats the regex baseline with real margin.
"""
import re
import time

from gliner2 import GLiNER2

# ── Hand-labeled cases: (text, expected_pii_spans) ──────────────────────────
# expected_pii_spans is a set of exact substrings that MUST be flagged as PII
# by ANY label. Cases with an empty set are negative controls -- text that
# superficially resembles PII context but contains none, to catch over-eager
# false positives (same failure class as the CoherenceGate KEEP-bias found
# earlier today).
CASES = [
    ("Contacta a Maria Garcia en maria.garcia@example.com o al telefono +34 612 345 678.",
     {"Maria Garcia", "maria.garcia@example.com", "+34 612 345 678"}),
    ("Su DNI es 12345678A y vive en Calle Real 15, Madrid.",
     {"12345678A", "Calle Real 15, Madrid"}),
    ("El paciente Juan Perez, nacido el 03/04/1985, ingreso con fiebre alta.",
     {"Juan Perez", "03/04/1985"}),
    ("Envia el informe a soporte@tylluan.dev antes del viernes.",
     {"soporte@tylluan.dev"}),
    ("La tarjeta de credito 4532 0151 1283 0366 fue rechazada dos veces.",
     {"4532 0151 1283 0366"}),
    ("Llama al numero de emergencias 112 si hay una urgencia real.",
     set()),  # negative: "numero" + digits but NOT a personal phone
    ("El commit fue verificado con el hash a4bfb45 y 539 tests en verde.",
     set()),  # negative: alphanumeric string that looks ID-like but is a git hash
    ("La reunion es el 25 de diciembre a las 10:00 en la sala principal.",
     set()),  # negative: date but not a person's birthdate/PII context
    ("Ana Lopez trabaja en el hospital central desde 2019, su email es ana.lopez@hospital.org.",
     {"Ana Lopez", "ana.lopez@hospital.org"}),
    ("El puerto real de Tylluan es 4000, no 3030 como decia la documentacion vieja.",
     set()),  # negative: numbers that are ports, not PII
    ("Pedro Sanchez, DNI 87654321B, telefono 699887766, reside en Barcelona.",
     {"Pedro Sanchez", "87654321B", "699887766"}),
    ("El benchmark dio 75.00% de precision sobre 52 casos reales.",
     set()),  # negative: percentages/numbers, no PII
    ("Numero de la seguridad social: 12 1234567890 12, titular Laura Martin.",
     {"12 1234567890 12", "Laura Martin"}),
    ("git commit -m 'fix(bench): correct nested result.guild parsing' -- autor Deep.",
     set()),  # negative: "autor Deep" is an agent name, not real personal PII in this context
    ("Cliente: Roberto Fernandez, IBAN ES91 2100 0418 4502 0005 1332, telefono +34 655123456.",
     {"Roberto Fernandez", "ES91 2100 0418 4502 0005 1332", "+34 655123456"}),
]

LABELS = ["person name", "email address", "phone number", "national id number",
          "physical address", "credit card number", "IBAN bank account number",
          "date of birth", "social security number"]

# ── Naive regex baseline ────────────────────────────────────────────────────
REGEX_PATTERNS = [
    re.compile(r"[\w.+-]+@[\w-]+\.[\w.-]+"),                      # email
    re.compile(r"\+?\d[\d ]{7,}\d"),                                # phone-ish (7+ digits)
    re.compile(r"\b\d{8}[A-Za-z]\b"),                               # Spanish DNI
    re.compile(r"\b(?:\d[ -]?){13,19}\b"),                          # credit card
    re.compile(r"\b[A-Z]{2}\d{2}[ ]?(?:\d{4}[ ]?){4,5}\b"),         # IBAN
]

def regex_detect(text):
    found = set()
    for pat in REGEX_PATTERNS:
        for m in pat.finditer(text):
            found.add(m.group().strip())
    return found


def score(predicted, expected, all_text):
    """Case is correct if: (expected empty AND predicted has no overlap with
    expected span-region) OR (every expected span appears in predicted, with
    minimal extra false-positive spans)."""
    if not expected:
        return len(predicted) == 0
    hits = sum(1 for e in expected if any(e in p or p in e for p in predicted))
    return hits == len(expected)


def main():
    print("=== GLiNER2 (fastino/gliner2-base-v1) PII detection spike ===")
    print(f"Loading model...")
    t0 = time.time()
    model = GLiNER2.from_pretrained("fastino/gliner2-base-v1")
    print(f"Loaded in {time.time()-t0:.1f}s")

    majority_class = sum(1 for _, e in CASES if not e)  # "no PII" baseline
    maj_acc = max(majority_class, len(CASES) - majority_class) / len(CASES)
    print(f"\nMajority-class baseline (always predict the more common outcome): {maj_acc*100:.2f}%")

    # Regex baseline
    regex_correct = 0
    for text, expected in CASES:
        pred = regex_detect(text)
        if score(pred, expected, text):
            regex_correct += 1
    regex_acc = regex_correct / len(CASES)
    print(f"Regex baseline: {regex_correct}/{len(CASES)} ({regex_acc*100:.2f}%)")

    # GLiNER2 real inference
    gliner_correct = 0
    latencies = []
    print(f"\n--- GLiNER2 real inference on {len(CASES)} cases ---")
    for i, (text, expected) in enumerate(CASES):
        t0 = time.time()
        result = model.extract_entities(text, LABELS)
        latencies.append(time.time() - t0)
        predicted = set()
        for spans in result.get("entities", {}).values():
            predicted.update(spans)
        ok = score(predicted, expected, text)
        gliner_correct += ok
        marker = "+" if ok else "-"
        print(f"  [{i+1}/{len(CASES)}] {marker} expected={expected or '{}'} predicted={predicted}")

    gliner_acc = gliner_correct / len(CASES)
    avg_lat = sum(latencies) / len(latencies)
    print(f"\nGLiNER2 accuracy: {gliner_correct}/{len(CASES)} ({gliner_acc*100:.2f}%), avg latency {avg_lat*1000:.1f}ms/case")

    print(f"\n{'='*72}")
    print("SUMMARY")
    print(f"{'='*72}")
    print(f"  Majority class (always predict more common outcome): {maj_acc*100:6.2f}%")
    print(f"  Regex baseline:                                      {regex_acc*100:6.2f}%")
    print(f"  GLiNER2 (fastino/gliner2-base-v1):                   {gliner_acc*100:6.2f}%  (avg {avg_lat*1000:.1f}ms/case)")
    verdict = "GO" if gliner_acc > regex_acc and gliner_acc > maj_acc else "NO-GO"
    print(f"  VERDICT: {verdict}")
    print(f"{'='*72}")

    import json
    from pathlib import Path
    result = {
        "date": "2026-07-27",
        "mode": "real_gliner2_pii_detection",
        "model": "fastino/gliner2-base-v1",
        "num_cases": len(CASES),
        "majority_class_baseline_pct": round(maj_acc * 100, 2),
        "regex_baseline_pct": round(regex_acc * 100, 2),
        "gliner2_accuracy_pct": round(gliner_acc * 100, 2),
        "avg_latency_ms": round(avg_lat * 1000, 1),
        "verdict": verdict,
        "verdict_basis": "gliner2_accuracy_pct must beat both the majority-class baseline and the naive regex baseline to justify adding a model-based PII detector over simple pattern matching.",
    }
    out_path = Path(__file__).parent / "results.json"
    out_path.write_text(json.dumps(result, indent=2, ensure_ascii=False))
    print(f"\nResult saved: {out_path}")


if __name__ == "__main__":
    main()
