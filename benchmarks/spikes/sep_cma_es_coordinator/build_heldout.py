"""Build held-out set from guild_audit_log for TRINITY sep-CMA-ES spike."""
import sqlite3
import json
import re
from pathlib import Path

DATA_DIR = Path("data")
AUDIT_DB = DATA_DIR / "audit.db"
OUT_FILE = Path("benchmarks/spikes/sep_cma_es_coordinator/heldout_set.json")

conn = sqlite3.connect(str(AUDIT_DB))
cur = conn.cursor()
cur.execute("""
    SELECT id, agent_id, guild, tool_name, intent, status, timestamp
    FROM guild_audit_log
    WHERE intent IS NOT NULL AND intent != ''
    ORDER BY id ASC
""")
rows = cur.fetchall()
conn.close()

connectors = re.compile(
    r'\b(?:then|and then|after that|finally|y luego|luego|despues|despu.s|finalmente)\b',
    re.IGNORECASE,
)
numbered = re.compile(r'\d+\.\s+\S')

multi_step = []
for row in rows:
    intent = row[4]
    if connectors.search(intent) or numbered.search(intent):
        multi_step.append({
            "id": row[0],
            "agent_id": row[1],
            "guild": row[2],
            "tool": row[3],
            "intent": intent,
            "status": row[5],
            "timestamp": row[6],
        })

print(f"Total audit rows: {len(rows)}")
print(f"Multi-step candidates: {len(multi_step)}")

# Now build scenarios. Each scenario is: { "intent": "...", "sub_tasks": [...], "expected_parallel": bool }
# Since we don't have ground-truth labels, we use the coordinator's own _split_intent() to
# generate sub-tasks, then run the fixed pipeline to establish a baseline.
#
# For the held-out set, we select diverse intents (different guilds, different sizes).

scenarios = []
seen_prefixes = set()

for m in multi_step:
    intent = m["intent"]
    # De-duplicate near-identical intents
    prefix = intent[:60].lower().strip()
    if prefix in seen_prefixes:
        continue
    seen_prefixes.add(prefix)

    # Parse out the sub-tasks using the same logic coordinator.py uses
    sub_tasks = []
    # Try split by connectors first
    parts = re.split(
        r"\s+(?:then|and then|after that|finally|y luego|luego|despues|despu.s|finalmente)\s+",
        intent,
        flags=re.IGNORECASE,
    )
    if len(parts) > 1:
        sub_tasks = [p.strip() for p in parts if p.strip()]
    else:
        # Try numbered list
        parts = re.split(r"\s*\d+\.\s+", intent)
        parts = [p.strip() for p in parts if p.strip()]
        if len(parts) > 1:
            sub_tasks = parts

    if len(sub_tasks) >= 2:
        scenarios.append({
            "intent": intent,
            "sub_tasks": sub_tasks,
            "agent_id": m["agent_id"],
            "source_guild": m["guild"],
        })

print(f"Scenarios with >=2 sub-tasks: {len(scenarios)}")

# Take up to 40 (20-50 range from ADR-010 plan)
scenarios = scenarios[:40]
print(f"Selected for held-out set: {len(scenarios)}")

# Split: 60% train (24), 40% held-out (16)
split = int(len(scenarios) * 0.6)
train = scenarios[:split]
heldout = scenarios[split:]

result = {
    "metadata": {
        "created": "2026-07-26",
        "source": str(AUDIT_DB),
        "total_audit_rows": len(rows),
        "multi_step_candidates": len(multi_step),
        "scenarios_selected": len(scenarios),
        "train_count": len(train),
        "heldout_count": len(heldout),
    },
    "train": train,
    "heldout": heldout,
}

OUT_FILE.parent.mkdir(parents=True, exist_ok=True)
with open(OUT_FILE, "w", encoding="utf-8") as f:
    json.dump(result, f, indent=2, ensure_ascii=False)

print(f"\nWritten to {OUT_FILE}")
print(f"  Train: {len(train)} scenarios")
print(f"  Held-out: {len(heldout)} scenarios")

# Print sample
print("\n--- Sample scenarios ---")
for i, s in enumerate(scenarios[:5]):
    print(f"\n[{i}] {s['agent_id']} via {s['source_guild']}:")
    print(f"    Intent: {s['intent'][:100]}...")
    print(f"    Sub-tasks: {s['sub_tasks']}")
