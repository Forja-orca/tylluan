"""D-144 (H3 de la auditoría de Buffy, 2026-11-07): formato de `run_at` del scheduler.

El fallo, tal cual: `run_at` se guardaba con `isoformat()`
(`2026-11-07T02:48:00.123456`, con `T`) y se comparaba como **texto** contra
`datetime('now')` (`2026-11-07 01:48:54`, con espacio). En ASCII `'T'` (0x54) >
`' '` (0x20), así que cualquier fila **del mismo día** quedaba "en el futuro" y un
`schedule(delay_minutes=60)` no vencía hasta el cambio de fecha: medianoche.

Los tests van contra `_due_rows()`, que es donde vive la decisión, con una conexión
SQLite en memoria y el mismo esquema que crea `_get_conn()` — sin tocar la BD real
del scheduler ni arrancar el hilo de polling.
"""

import pathlib
import re
import sqlite3
import sys
from datetime import datetime, timedelta, timezone

sys.path.insert(0, str(pathlib.Path(__file__).resolve().parent.parent.parent))

from guilds.core.scheduler import _due_rows, _run_at_for

ESQUEMA = """
CREATE TABLE schedules (
    id TEXT PRIMARY KEY,
    intent TEXT NOT NULL,
    agent_id TEXT NOT NULL,
    channel TEXT DEFAULT 'schedules',
    run_at TEXT NOT NULL,
    created_at TEXT DEFAULT (datetime('now')),
    fired INTEGER DEFAULT 0,
    error TEXT
)
"""


def _conn() -> sqlite3.Connection:
    c = sqlite3.connect(":memory:")
    c.row_factory = sqlite3.Row
    c.execute(ESQUEMA)
    return c


def _insertar(conn: sqlite3.Connection, sid: str, run_at: str, fired: int = 0) -> None:
    conn.execute(
        "INSERT INTO schedules (id, intent, agent_id, channel, run_at, fired) "
        "VALUES (?, ?, ?, ?, ?, ?)",
        (sid, "despertar", "deep", "general", run_at, fired),
    )
    conn.commit()


def _formato_viejo(minutos: int) -> str:
    """Lo que guardaba el scheduler antes del arreglo: isoformat() con `T`."""
    return (datetime.now(timezone.utc) - timedelta(minutes=minutos)).isoformat()


def _formato_nuevo(minutos: int) -> str:
    """Lo que guarda desde el arreglo: ISO de SQLite, con espacio."""
    return (datetime.now(timezone.utc) - timedelta(minutes=minutos)).strftime(
        "%Y-%m-%d %H:%M:%S"
    )


def _ids(conn: sqlite3.Connection) -> set[str]:
    return {fila["id"] for fila in _due_rows(conn)}


def test_d144_una_fila_vencida_hace_5_min_se_dispara():
    """El caso que el TL pidió literalmente: vencida hace 5 min, en formato viejo."""
    conn = _conn()
    _insertar(conn, "vencida", _formato_viejo(5))
    assert _ids(conn) == {"vencida"}, (
        "una fila de hace 5 minutos (formato con 'T') tiene que estar vencida: si no, "
        "el scheduler la deja para el cambio de fecha"
    )


def test_d144_una_fila_a_60_min_no_se_dispara_todavia():
    """El escenario del fallo: a 60 min no vence antes de tiempo (ni de menos)."""
    conn = _conn()
    _insertar(conn, "dentro_de_una_hora", _formato_nuevo(-60))
    assert _ids(conn) == set(), "una tarea a 60 min en el futuro no puede dispararse ya"


def test_d144_el_formato_nuevo_tambien_esta_vencido():
    """El arreglo no se apoya solo en el formato: la fila nueva también vence."""
    conn = _conn()
    _insertar(conn, "nueva_vencida", _formato_nuevo(5))
    assert _ids(conn) == {"nueva_vencida"}


def test_d144_una_fila_ya_disparada_no_vuelve():
    conn = _conn()
    _insertar(conn, "ya_fue", _formato_viejo(30), fired=1)
    assert _ids(conn) == set(), "una fila con fired=1 no puede volver a dispararse"


def test_d144_el_alta_guarda_el_formato_de_sqlite():
    """Y el otro lado del contrato: lo que se guarda ya no lleva `T` ni fracciones."""
    run_at = _run_at_for(60)
    assert re.fullmatch(r"\d{4}-\d\d-\d\d \d\d:\d\d:\d\d", run_at), (
        f"run_at tiene que ser ISO de SQLite ('YYYY-MM-DD HH:MM:SS'), no {run_at!r}"
    )
    cuando = datetime.strptime(run_at, "%Y-%m-%d %H:%M:%S")
    ahora = datetime.now(timezone.utc).replace(tzinfo=None)
    minutos = (cuando - ahora).total_seconds() / 60
    assert 59 <= minutos <= 61, f"delay de 60 min esperado, salieron {minutos:.1f}"
