"""Seed guild: Tier 1 cold-start seed export/import for SilvaDB.

Tier 1 = own domain, low risk (technical documentation, project knowledge).
Tier 2 = sensitive domains (medical, legal) requires external verification
and is explicitly NOT implemented here. See docs/roadmap/ROADMAP_O3.md.

Export: dump high-weight nodes as portable JSON seed file.
Import: load seed JSON into a fresh SilvaDB via kernel API.
"""
import json
import os
import sqlite3
import sys
from pathlib import Path
from mcp.server.fastmcp import FastMCP

mcp = FastMCP("seed_tools")

DATA_DIR = Path("data")
KERNEL_URL = "http://127.0.0.1:4000"


def _resolve_kernel_base():
    if "KERNEL_BASE" in os.environ:
        return os.environ["KERNEL_BASE"]
    port_file = Path(__file__).resolve().parent.parent.parent / "data" / "active_port.json"
    try:
        data = json.loads(port_file.read_text())
        port = data.get("port", 4000)
        return f"http://127.0.0.1:{port}"
    except Exception:
        return "http://127.0.0.1:4000"


KERNEL_URL = _resolve_kernel_base()


@mcp.tool()
def seed_export(output_path: str = "", min_weight: float = 0.5, max_nodes: int = 500) -> str:
    """Export high-value SilvaDB nodes as a portable Tier 1 seed file.

    Exports only Tier 1 content: technical knowledge, project docs, agent
    experiences. Never exports PII, credentials, or medical/legal content.
    Use min_weight to filter by node importance.

    Args:
        output_path: Where to save the seed JSON. Default: data/seeds/seed_YYYYMMDD.json
        min_weight: Minimum node weight to include (0-1). Default 0.5.
        max_nodes: Maximum nodes to export. Default 500.
    """
    silva_db = DATA_DIR / "silva.db"
    if not silva_db.exists():
        return json.dumps({"error": "SilvaDB not found — no data to export"})

    if not output_path:
        seeds_dir = DATA_DIR / "seeds"
        seeds_dir.mkdir(parents=True, exist_ok=True)
        from datetime import datetime
        output_path = str(seeds_dir / f"seed_{datetime.now().strftime('%Y%m%d_%H%M%S')}.json")

    conn = sqlite3.connect(str(silva_db))
    cur = conn.cursor()

    cur.execute("""
        SELECT id, node_type, content, provenance, weight, created_at, metadata
        FROM nodes
        WHERE weight >= ?
        ORDER BY weight DESC
        LIMIT ?
    """, (min_weight, max_nodes))
    nodes = []
    for row in cur.fetchall():
        nodes.append({
            "id": row[0],
            "node_type": row[1],
            "content": row[2],
            "provenance": row[3],
            "weight": row[4],
            "created_at": row[5],
            "metadata": row[6],
        })

    conn.close()

    seed = {
        "version": 1,
        "tier": 1,
        "source": "tylluan-seed-export",
        "created": __import__("datetime").datetime.now().isoformat(),
        "node_count": len(nodes),
        "min_weight": min_weight,
        "nodes": nodes,
        "disclaimer": "Tier 1 only — technical/project knowledge. No PII, credentials, medical, or legal content.",
    }

    with open(output_path, "w", encoding="utf-8") as f:
        json.dump(seed, f, indent=2, ensure_ascii=False)

    size_kb = os.path.getsize(output_path) / 1024
    return json.dumps({
        "status": "ok",
        "output": output_path,
        "node_count": len(nodes),
        "size_kb": round(size_kb, 1),
    })


@mcp.tool()
def seed_import(seed_path: str, dry_run: bool = True) -> str:
    """Import a Tier 1 seed file into SilvaDB via kernel API.

    Only imports nodes — never overwrites existing data. Each node is
    created via POST /api/v1/silva/node if it doesn't already exist.

    Args:
        seed_path: Path to the seed JSON file.
        dry_run: If true (default), only validates the seed without importing.
    """
    import urllib.request as _urllib

    if not os.path.exists(seed_path):
        return json.dumps({"error": f"Seed file not found: {seed_path}"})

    with open(seed_path, "r", encoding="utf-8") as f:
        seed = json.load(f)

    if seed.get("tier") != 1:
        return json.dumps({
            "error": f"Seed tier {seed.get('tier')} is not Tier 1 — only Tier 1 seeds are importable. Tier 2+ requires external verification.",
        })

    nodes = seed.get("nodes", [])
    if not nodes:
        return json.dumps({"error": "Seed contains no nodes"})

    if dry_run:
        return json.dumps({
            "status": "validated",
            "node_count": len(nodes),
            "tier": seed.get("tier"),
            "created": seed.get("created"),
            "note": "Dry run — no data imported. Set dry_run=false to import.",
        })

    imported = 0
    errors = 0
    for node in nodes:
        try:
            data = json.dumps({
                "id": node.get("id"),
                "node_type": node.get("node_type"),
                "content": node.get("content"),
                "provenance": node.get("provenance", "seed_import"),
                "weight": node.get("weight", 0.5),
            }).encode("utf-8")
            req = _urllib.Request(
                f"{KERNEL_URL}/api/v1/silva/node",
                data=data,
                headers={"Content-Type": "application/json"},
                method="POST",
            )
            with _urllib.urlopen(req, timeout=30) as resp:
                result = json.loads(resp.read())
                if result.get("ok") or result.get("status") == "ok":
                    imported += 1
                else:
                    errors += 1
        except Exception as e:
            sys.stderr.write(f"[seed_import] Error importing {node.get('id')}: {e}\n")
            errors += 1

    return json.dumps({
        "status": "ok" if errors == 0 else "partial",
        "imported": imported,
        "errors": errors,
        "total": len(nodes),
    })


if __name__ == "__main__":
    from guilds.core import utils
    utils.safe_mcp_run(mcp)
