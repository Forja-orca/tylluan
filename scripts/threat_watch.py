#!/usr/bin/env python3
"""Vigía semanal de amenazas de Tylluan (prototipo). Solo biblioteca estandar."""
from __future__ import annotations

import argparse
import json
import re
import sys
import urllib.request
from datetime import date, datetime, timedelta, timezone
from pathlib import Path
from typing import Callable, NamedTuple

KEV_URL = "https://www.cisa.gov/sites/default/files/feeds/known_exploited_vulnerabilities.json"
OSV_URL = "https://api.osv.dev/v1/querybatch"
OSV_BATCH_SIZE = 500
TIMEOUT_SECS = 30
SHOWN_ITEMS = 3
ECO_CARGO = "crates.io"
ECO_NPM = "npm"
ECO_PYPI = "PyPI"
MATCH_NOTE = "posible, revisar a mano"
EMPTY_SLOT = "  -"

Package = tuple[str, str, str]  # (ecosistema, nombre, version)

_CARGO_NAME = re.compile(r'^name = "([^"]+)"\s*$')
_CARGO_VERSION = re.compile(r'^version = "([^"]+)"\s*$')
_PNPM_KEY = re.compile(r"^  '?([^\s'].*?)@(\d[^'\s:()]*)'?:\s*$")
_PIP_PIN = re.compile(r"^([A-Za-z0-9][A-Za-z0-9._-]*)==([^\s\\;]+)")
INVENTORY_FILES = ("Cargo.lock", "dashboard/pnpm-lock.yaml", "guilds/requirements-hashes.txt")


class OsvResult(NamedTuple):
    ok: bool
    hits: list  # [(ecosistema, nombre, version, id_aviso)]
    error: str | None
    truncated: bool


# --- KEV ---------------------------------------------------------------

def fetch_kev(url: str = KEV_URL) -> dict:
    """Descarga el catálogo KEV. Lanza OSError/ValueError si falla."""
    with urllib.request.urlopen(url, timeout=TIMEOUT_SECS) as resp:
        data = json.loads(resp.read().decode("utf-8"))
    if not isinstance(data, dict) or not isinstance(data.get("vulnerabilities"), list):
        raise ValueError("KEV sin lista 'vulnerabilities'")
    return data


def _parse_date(text: object) -> date | None:
    try:
        return datetime.strptime(str(text), "%Y-%m-%d").date()
    except ValueError:
        return None


def kev_last_days(data: dict, days: int, today: date) -> list[dict]:
    """Entradas KEV con dateAdded en [today - days, today]."""
    start = today - timedelta(days=days)
    recent = []
    for item in data.get("vulnerabilities", []):
        added = _parse_date(item.get("dateAdded"))
        if added is not None and start <= added <= today:
            recent.append(item)
    return recent


# --- Inventario --------------------------------------------------------

def parse_cargo_lock(text: str) -> list[Package]:
    packages: list[Package] = []
    name: str | None = None
    in_package = False
    for line in text.splitlines():
        if line.strip() == "[[package]]":
            name, in_package = None, True
            continue
        if line.startswith("[") and in_package:
            in_package = False
        if not in_package:
            continue
        name_match = _CARGO_NAME.match(line)
        if name_match:
            name = name_match.group(1)
            continue
        version_match = _CARGO_VERSION.match(line)
        if version_match and name:
            packages.append((ECO_CARGO, name, version_match.group(1)))
            name, in_package = None, False
    return packages


def parse_pnpm_lock(text: str) -> list[Package]:
    packages: list[Package] = []
    in_section = False
    for line in text.splitlines():
        if line.startswith("packages:"):
            in_section = True
            continue
        if in_section and line and not line.startswith(" "):
            break
        key_match = _PNPM_KEY.match(line) if in_section else None
        if key_match:
            packages.append((ECO_NPM, key_match.group(1), key_match.group(2)))
    return packages


def parse_package_json(text: str) -> list[Package]:
    """Respaldo sin lock: versiones de rango (^ o ~) sin resolver."""
    data = json.loads(text)
    deps = {**data.get("dependencies", {}), **data.get("devDependencies", {})}
    packages: list[Package] = []
    for name, spec in deps.items():
        version = str(spec).lstrip("^~")
        if version[:1].isdigit():
            packages.append((ECO_NPM, name, version))
    return packages


def parse_requirements(text: str) -> list[Package]:
    packages: list[Package] = []
    for line in text.splitlines():
        pin = _PIP_PIN.match(line)
        if pin:
            packages.append((ECO_PYPI, pin.group(1).lower(), pin.group(2)))
    return packages


def _read(path: Path) -> str:
    return path.read_text(encoding="utf-8", errors="replace")


def inventory(repo: Path) -> list[Package]:
    """Paquetes unicos de Cargo.lock, pnpm-lock (o package.json) y requirements."""
    found: list[Package] = []
    cargo = repo / "Cargo.lock"
    if cargo.is_file():
        found += parse_cargo_lock(_read(cargo))
    pnpm = repo / "dashboard" / "pnpm-lock.yaml"
    package_json = repo / "dashboard" / "package.json"
    if pnpm.is_file():
        found += parse_pnpm_lock(_read(pnpm))
    elif package_json.is_file():
        found += parse_package_json(_read(package_json))
    reqs = repo / "guilds" / "requirements-hashes.txt"
    if reqs.is_file():
        found += parse_requirements(_read(reqs))
    return sorted(set(found))


def inventory_gaps(repo: Path) -> list[str]:
    """Limitaciones de inventario: ficheros esperados que no existen."""
    if not repo.is_dir():
        return ["repo no encontrado"]
    gaps = [f"sin {rel}" for rel in INVENTORY_FILES if not (repo / rel).is_file()]
    if not (repo / "dashboard" / "pnpm-lock.yaml").is_file():
        gaps.append("npm desde package.json (sin lock)")
    return gaps


# --- OSV ---------------------------------------------------------------

def _http_post(url: str, payload: dict) -> dict:
    body = json.dumps(payload).encode("utf-8")
    request = urllib.request.Request(
        url, data=body, headers={"Content-Type": "application/json"}, method="POST"
    )
    with urllib.request.urlopen(request, timeout=TIMEOUT_SECS) as resp:
        return json.loads(resp.read().decode("utf-8"))


def osv_query(
    inv: list[Package],
    post: Callable[[str, dict], dict] = _http_post,
    batch_size: int = OSV_BATCH_SIZE,
) -> OsvResult:
    """Consulta OSV por lotes. Ante cualquier error devuelve ok=False (fuente no leída)."""
    hits: list = []
    truncated = False
    for start in range(0, len(inv), batch_size):
        chunk = inv[start:start + batch_size]
        payload = {"queries": [
            {"package": {"name": name, "ecosystem": eco}, "version": version}
            for eco, name, version in chunk
        ]}
        try:
            response = post(OSV_URL, payload)
        except (OSError, ValueError) as exc:
            return OsvResult(False, hits, f"{type(exc).__name__}: {exc}", truncated)
        results = response.get("results") if isinstance(response, dict) else None
        if not isinstance(results, list) or len(results) != len(chunk):
            return OsvResult(False, hits, "respuesta OSV sin 'results' completos", truncated)
        for package, result in zip(chunk, results):
            truncated = truncated or bool(result.get("next_page_token"))
            for vuln in result.get("vulns") or []:
                hits.append((*package, vuln.get("id", "?")))
    return OsvResult(True, hits, None, truncated)


def offline_osv_post(fixture: dict) -> Callable[[str, dict], dict]:
    """Simula OSV con un fixture {"ecosistema:nombre@version": [ids]}."""
    def post(_url: str, payload: dict) -> dict:
        results = []
        for query in payload["queries"]:
            key = (f"{query['package']['ecosystem']}:{query['package']['name']}"
                   f"@{query['version']}")
            results.append({"vulns": [{"id": vid} for vid in fixture.get(key, [])]})
        return {"results": results}
    return post


# --- Cruce y render ----------------------------------------------------

def match_kev(kev_items: list[dict], inv: list[Package]) -> list[dict]:
    """Coincidencia simple: nombre de producto KEV (minusculas) = nombre de paquete."""
    by_name: dict[str, list[Package]] = {}
    for eco, name, version in inv:
        by_name.setdefault(name.lower(), []).append((eco, name, version))
    matches = []
    for item in kev_items:
        product = str(item.get("product", "")).strip().lower()
        for eco, name, version in by_name.get(product, []):
            matches.append({
                "cveID": item.get("cveID", "?"),
                "product": item.get("product", ""),
                "ecosystem": eco,
                "name": name,
                "version": version,
                "aviso": MATCH_NOTE,
            })
    return matches


def _one_line(text: object) -> str:
    return " ".join(str(text).split())


def _pad(rows: list[str]) -> list[str]:
    shown = rows[:SHOWN_ITEMS]
    return shown + [EMPTY_SLOT] * (SHOWN_ITEMS - len(shown))


def _kev_row(item: dict) -> str:
    return _one_line(f"  {item.get('cveID', '?')} | {item.get('vendorProject', '')} "
                     f"{item.get('product', '')} | {item.get('dateAdded', '')}")


def _osv_row(hit: tuple) -> str:
    eco, name, version, vid = hit
    return _one_line(f"  {vid} | {eco} {name} {version}")


def _action(failed: list[str], matches: list[dict], osv_hits: list) -> str:
    if failed:
        return "repetir la lectura de las fuentes no leídas antes de cerrar el ciclo"
    if matches:
        return "revisar a mano las coincidencias y actualizar la dependencia afectada"
    if osv_hits:
        return "revisar los avisos OSV de dependencias"
    return "sin acción urgente; repetir la próxima semana"


def render_report(
    *,
    today: date,
    days: int,
    kev_new: list[dict] | None,
    osv: OsvResult | None,
    matches: list[dict],
    read_times: dict[str, str],
    failed: list[str],
    limitations: list[str],
) -> str:
    """Informe de exactamente 15 lineas (kev_new/osv None = fuente no leída)."""
    iso_year, iso_week, _ = today.isocalendar()
    lines = [f"SEMANA {iso_year}-{iso_week:02d} - Amenazas Tylluan"]

    if kev_new is None:
        lines.append("KEV nuevos: fuente no leída")
        lines += [EMPTY_SLOT] * SHOWN_ITEMS
    else:
        lines.append(f"KEV nuevos: {len(kev_new)} (últimos {days} días)")
        lines += _pad([_kev_row(item) for item in kev_new])

    osv_ok = osv is not None and osv.ok
    if osv_ok:
        lines.append(f"Avisos OSV que tocan dependencias: {len(osv.hits)}")
        lines += _pad([_osv_row(hit) for hit in osv.hits])
    else:
        lines.append("Avisos OSV que tocan dependencias: fuente no leída")
        lines += [EMPTY_SLOT] * SHOWN_ITEMS

    first = matches[0] if matches else None
    if first:
        extra = f" (+{len(matches) - 1} más)" if len(matches) > 1 else ""
        row = _one_line(f"  {first['cveID']} ~ {first['name']} {first['version']}{extra}")
    else:
        row = EMPTY_SLOT
    lines.append(f"Coincidencias con inventario: {len(matches)} ({MATCH_NOTE})")
    lines.append(row)

    read = ", ".join(f"{src} {when}" for src, when in read_times.items()) or "ninguna"
    lines.append(f"Fuentes leídas (UTC): {read}")
    lines.append(f"Fuentes NO leídas: {'; '.join(failed) if failed else 'ninguna'}")
    lines.append(f"Limitaciones: {'; '.join(limitations) if limitations else 'ninguna conocida'}")
    lines.append(f"Acción recomendada: {_action(failed, matches, osv.hits if osv_ok else [])}")

    return "\n".join(_one_line(line) for line in lines)


# --- Orquestacion y CLI ------------------------------------------------

def _utc_now() -> str:
    return datetime.now(timezone.utc).strftime("%Y-%m-%d %H:%MZ")


def _load_kev(offline_dir: Path | None, days: int, today: date) -> list[dict]:
    if offline_dir is None:
        return kev_last_days(fetch_kev(), days, today)
    data = json.loads((offline_dir / "kev.json").read_text(encoding="utf-8"))
    return kev_last_days(data, days, today)


def build_report(repo: Path, days: int, today: date, offline_dir: Path | None = None) -> str:
    failed: list[str] = []
    read_times: dict[str, str] = {}
    limitations: list[str] = []

    kev_new: list[dict] | None = None
    try:
        kev_new = _load_kev(offline_dir, days, today)
        read_times["KEV"] = _utc_now()
    except (OSError, ValueError) as exc:
        failed.append(f"KEV ({type(exc).__name__})")

    inv = inventory(repo)
    read_times["inventario"] = _utc_now()
    limitations += inventory_gaps(repo)

    osv: OsvResult | None
    try:
        post = offline_osv_post(json.loads(
            (offline_dir / "osv_by_package.json").read_text(encoding="utf-8")
        )) if offline_dir else _http_post
        osv = osv_query(inv, post=post)
    except OSError as exc:
        osv = OsvResult(False, [], f"{type(exc).__name__}: {exc}", False)
    if osv.ok:
        read_times["OSV"] = _utc_now()
    else:
        failed.append(f"OSV ({osv.error})")
    if osv.truncated:
        limitations.append("OSV sin seguir paginación")

    limitations.append("KEV cruzado solo por nombre de producto")
    matches = match_kev(kev_new, inv) if kev_new is not None else []
    return render_report(
        today=today, days=days, kev_new=kev_new, osv=osv, matches=matches,
        read_times=read_times, failed=failed, limitations=limitations,
    )


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description="Vigía semanal de amenazas de Tylluan")
    parser.add_argument("--repo", type=Path, default=Path("."), help="raiz del repo a inventariar")
    parser.add_argument("--days", type=int, default=7, help="ventana KEV en dias")
    parser.add_argument("--out", type=Path, default=None, help="fichero del informe (omitir = stdout)")
    parser.add_argument("--offline-fixture", type=Path, default=None,
                        help="directorio con kev.json y osv_by_package.json (sin red)")
    args = parser.parse_args(argv)
    report = build_report(args.repo, args.days, date.today(), args.offline_fixture)
    if args.out:
        args.out.write_text(report + "\n", encoding="utf-8")
    else:
        sys.stdout.buffer.write((report + "\n").encode("utf-8"))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
