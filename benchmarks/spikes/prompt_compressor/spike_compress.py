"""Spike: Prompt compression viability — measure token savings on real intents."""
import json, sqlite3

conn = sqlite3.connect("data/audit.db")
rows = conn.execute("""
    SELECT DISTINCT intent FROM guild_audit_log
    WHERE intent IS NOT NULL AND intent != ''
    AND LENGTH(intent) > 20
    ORDER BY LENGTH(intent) DESC
    LIMIT 100
""").fetchall()
conn.close()

intents = [r[0] for r in rows if r[0]]

# Simple stop-word removal simulating what a small compressor model could do
STOP_WORDS = {
    "the", "a", "an", "is", "are", "was", "were", "be", "been", "being",
    "have", "has", "had", "do", "does", "did", "will", "would", "could",
    "should", "may", "might", "can", "shall", "to", "of", "in", "for",
    "on", "with", "at", "by", "from", "as", "into", "through", "during",
    "before", "after", "above", "below", "between", "under", "again",
    "further", "then", "once", "here", "there", "when", "where", "why",
    "how", "all", "both", "each", "few", "more", "most", "other", "some",
    "such", "no", "nor", "not", "only", "own", "same", "so", "than",
    "too", "very", "just", "about", "now", "also", "very", "really",
    "actually", "basically", "literally", "simply",
    "el", "la", "los", "las", "un", "una", "unos", "unas", "de", "del",
    "en", "con", "por", "para", "es", "son", "esta", "este", "esto",
    "que", "y", "o", "pero", "si", "no", "me", "te", "se", "lo", "le",
    "su", "mi", "tu", "al", "del",
}

def compress(intent):
    """Simple stop-word + short-word removal."""
    words = intent.split()
    kept = []
    for w in words:
        if w.lower() in STOP_WORDS or len(w) <= 1:
            continue
        kept.append(w)
    return " ".join(kept)

total_before = 0
total_after = 0
results = []

for intent in intents:
    before = len(intent.split())
    compressed = compress(intent)
    after = len(compressed.split()) if compressed else 0
    if before > 0 and after > 0:
        ratio = after / before
        total_before += before
        total_after += after
        results.append({
            "intent": intent[:100],
            "before": before,
            "after": after,
            "ratio": round(ratio, 3),
        })

avg_ratio = total_after / total_before if total_before > 0 else 0
savings = (1 - avg_ratio) * 100

print(f"Sample size: {len(results)} intents")
print(f"Total tokens before: {total_before}")
print(f"Total tokens after:  {total_after}")
print(f"Compression ratio: {avg_ratio:.1%} (keep {avg_ratio*100:.0f}% of tokens)")
print(f"Token savings: {savings:.0f}%")
print()
print("Top 5 intents:")
for r in sorted(results, key=lambda x: x["ratio"])[:5]:
    print(f"  {r['ratio']:.0%} keep | {r['before']} -> {r['after']} tokens | {r['intent'][:80]}")
print()
print("Bottom 5 intents (least savings):")
for r in sorted(results, key=lambda x: -x["ratio"])[:5]:
    print(f"  {r['ratio']:.0%} keep | {r['before']} -> {r['after']} tokens | {r['intent'][:80]}")

out = {
    "samples": len(results),
    "total_before": total_before,
    "total_after": total_after,
    "compression_ratio": round(avg_ratio, 3),
    "token_savings_pct": round(savings, 1),
}
with open("benchmarks/spikes/prompt_compressor/compression_baseline.json", "w") as f:
    json.dump(out, f, indent=2)
print(f"\nSaved to benchmarks/spikes/prompt_compressor/compression_baseline.json")
