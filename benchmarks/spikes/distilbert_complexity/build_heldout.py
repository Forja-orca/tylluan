"""Build held-out intents for DistilBERT complexity classification benchmark."""
import sqlite3, json, random
from pathlib import Path

conn = sqlite3.connect("data/audit.db")
rows = conn.execute("""
    SELECT DISTINCT intent FROM guild_audit_log
    WHERE intent IS NOT NULL AND intent != ''
    AND guild NOT IN ('coloquio', 'kernel')
    ORDER BY LENGTH(intent) DESC
""").fetchall()
conn.close()

intents = [r[0] for r in rows if r[0] and len(r[0]) > 5 and len(r[0]) < 500]
random.seed(42)

short = [i for i in intents if len(i) < 30]
med = [i for i in intents if 30 <= len(i) <= 80]
long = [i for i in intents if len(i) > 80]

selected = (
    random.sample(short, min(20, len(short)))
    + random.sample(med, min(20, len(med)))
    + random.sample(long, min(15, len(long)))
)

out_dir = Path("benchmarks/spikes/distilbert_complexity")
out_dir.mkdir(parents=True, exist_ok=True)

out = {"intents": selected, "count": len(selected)}
with open(out_dir / "heldout_intents.json", "w", encoding="utf-8") as f:
    json.dump(out, f, indent=2, ensure_ascii=False)

print(f"Total distinct: {len(intents)}")
print(f"Short: {len(short)}, Med: {len(med)}, Long: {len(long)}")
print(f"Held-out: {len(selected)}")
