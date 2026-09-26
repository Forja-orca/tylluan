import json
import sqlite3
from pathlib import Path

SPIKE_DIR = Path(__file__).resolve().parent
DB_PATH = SPIKE_DIR.parent.parent.parent / "data" / "mailbox.db"
OUT_FILE = SPIKE_DIR / "digest_cases_heldout.json"

def build_heldout():
    SPIKE_DIR.mkdir(parents=True, exist_ok=True)
    conn = sqlite3.connect(str(DB_PATH))
    cursor = conn.cursor()
    
    # Query distinct blocks of messages across active channels
    channels = ["mision-activa", "equipo", "general", "dev"]
    cases = []
    
    # Let's extract 25 realistic episodes of 3-6 turns with high information density
    for ch in channels:
        rows = cursor.execute(
            "SELECT msg_id, author_id, turn, content FROM coloquio_messages WHERE channel_id=? AND length(content) > 60 ORDER BY turn ASC",
            (ch,)
        ).fetchall()
        
        step = max(1, len(rows) // 7)
        for i in range(0, len(rows) - 3, step):
            if len(cases) >= 25:
                break
            chunk = rows[i:i+4]
            case_id = f"digest_case_{len(cases)+1:02d}_{ch}"
            msgs = []
            authors = []
            for r in chunk:
                auth = r[1]
                if auth not in authors:
                    authors.append(auth)
                msgs.append({
                    "msg_id": r[0],
                    "author": auth,
                    "turn": r[2],
                    "content": r[3]
                })
            
            # Identify key facts/decisions from the chunk
            key_terms = []
            for m in msgs:
                txt = m["content"]
                # Extract salient tokens / terms
                if "commit" in txt.lower():
                    key_terms.append("commit")
                if "test" in txt.lower() or "tests" in txt.lower():
                    key_terms.append("test")
                if "kernel" in txt.lower():
                    key_terms.append("kernel")
                if "port" in txt.lower() or "47004" in txt or "4000" in txt:
                    key_terms.append("port")
                if "clippy" in txt.lower():
                    key_terms.append("clippy")
                if "verificacion" in txt.lower() or "verificado" in txt.lower():
                    key_terms.append("verificacion")
                if "spike" in txt.lower():
                    key_terms.append("spike")
                if "router" in txt.lower() or "scheduler" in txt.lower():
                    key_terms.append("scheduler")
            
            dedup_terms = list(dict.fromkeys(key_terms))[:4]
            if not dedup_terms:
                dedup_terms = ["coloquio", "tarea"]
                
            cases.append({
                "id": case_id,
                "channel_id": ch,
                "turns": [m["turn"] for m in msgs],
                "authors": authors,
                "messages": msgs,
                "key_facts_required": dedup_terms
            })
            
    conn.close()
    
    with open(OUT_FILE, "w", encoding="utf-8") as f:
        json.dump(cases, f, indent=2, ensure_ascii=False)
        
    print(f"Saved {len(cases)} held-out Coloquio digest episodes to {OUT_FILE}")

if __name__ == "__main__":
    build_heldout()
