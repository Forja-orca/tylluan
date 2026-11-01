"""Pruebas sin red: todas las fuentes externas se simulan o se leen de fixtures."""
import json
import urllib.error
from datetime import date
from pathlib import Path

import sys

sys.path.insert(0, str(Path(__file__).resolve().parents[2] / "scripts"))

import threat_watch as tw  # noqa: E402

FIXTURES = Path(__file__).parent / "fixtures_threat_watch"
TODAY = date(2026, 11, 1)
REPORT_LINES = 15


def _load_kev_fixture() -> dict:
    return json.loads((FIXTURES / "kev.json").read_text(encoding="utf-8"))


def test_kev_filter_keeps_only_last_days_and_ignores_bad_dates():
    cves = [item["cveID"] for item in tw.kev_last_days(_load_kev_fixture(), 7, TODAY)]
    assert cves == ["CVE-2099-0001", "CVE-2099-0002"]


def test_kev_window_is_inclusive_on_both_ends():
    data = {"vulnerabilities": [
        {"cveID": "A", "dateAdded": "2026-10-25"},
        {"cveID": "B", "dateAdded": "2026-10-24"},
        {"cveID": "C", "dateAdded": "2026-11-01"},
        {"cveID": "D", "dateAdded": "2026-11-02"},
    ]}
    assert [item["cveID"] for item in tw.kev_last_days(data, 7, TODAY)] == ["A", "C"]


def test_parse_cargo_lock_fixture_reads_name_and_version_only():
    text = (FIXTURES / "repo" / "Cargo.lock").read_text(encoding="utf-8")
    assert tw.parse_cargo_lock(text) == [
        ("crates.io", "serde", "1.0.200"),
        ("crates.io", "tokio", "1.40.0"),
    ]


def test_inventory_covers_three_ecosystems_and_reports_no_gaps():
    repo = FIXTURES / "repo"
    inv = tw.inventory(repo)
    assert {eco for eco, _, _ in inv} == {"crates.io", "npm", "PyPI"}
    assert ("npm", "@tanstack/react-virtual", "3.14.21") in inv
    assert ("PyPI", "requests", "2.32.3") in inv
    assert tw.inventory_gaps(repo) == []


def test_match_kev_flags_candidates_for_manual_review():
    inv = [("crates.io", "serde", "1.0.200")]
    matches = tw.match_kev(_load_kev_fixture()["vulnerabilities"][:1], inv)
    assert matches == [{
        "cveID": "CVE-2099-0001", "product": "serde", "ecosystem": "crates.io",
        "name": "serde", "version": "1.0.200", "aviso": tw.MATCH_NOTE,
    }]
    assert "revisar a mano" in tw.MATCH_NOTE


def test_osv_queries_are_sent_in_batches_of_500():
    calls = []

    def fake_post(_url, payload):
        calls.append(len(payload["queries"]))
        return {"results": [{} for _ in payload["queries"]]}

    inv = [("npm", f"pkg{i}", "1.0.0") for i in range(1200)]
    result = tw.osv_query(inv, post=fake_post)
    assert calls == [500, 500, 200]
    assert result.ok is True and result.hits == []


def test_osv_network_failure_is_reported_not_raised():
    def failing_post(_url, _payload):
        raise urllib.error.URLError("sin red")

    result = tw.osv_query([("npm", "react", "19.2.0")], post=failing_post)
    assert result.ok is False
    assert "URLError" in result.error


def test_report_marks_osv_as_not_read_when_it_fails():
    failed_osv = tw.OsvResult(False, [], "URLError: sin red", False)
    report = tw.render_report(
        today=TODAY, days=7, kev_new=[], osv=failed_osv, matches=[],
        read_times={"KEV": "2026-11-01 03:00Z"}, failed=["OSV (URLError: sin red)"],
        limitations=[],
    )
    lines = report.splitlines()
    assert len(lines) == REPORT_LINES
    assert lines[5] == "Avisos OSV que tocan dependencias: fuente no leída"
    assert lines[12] == "Fuentes NO leídas: OSV (URLError: sin red)"


def test_report_has_exactly_15_lines_with_many_items():
    kev_items = [{"cveID": f"CVE-2099-{i:04d}", "vendorProject": "V", "product": "p",
                  "dateAdded": "2026-10-30"} for i in range(9)]
    osv = tw.OsvResult(True, [("npm", "react", "19.2.0", f"GHSA-{i}") for i in range(7)],
                       None, False)
    matches = tw.match_kev(kev_items, [("npm", "p", "1.0.0")])
    report = tw.render_report(
        today=TODAY, days=7, kev_new=kev_items, osv=osv, matches=matches,
        read_times={"KEV": "t1", "OSV": "t2", "inventario": "t3"}, failed=[],
        limitations=["x"],
    )
    lines = report.splitlines()
    assert len(lines) == REPORT_LINES
    assert lines[0] == "SEMANA 2026-44 - Amenazas Tylluan"
    assert lines[1] == "KEV nuevos: 9 (últimos 7 días)"
    assert lines[5] == "Avisos OSV que tocan dependencias: 7"
    assert lines[14].startswith("Acción recomendada: revisar a mano")


def test_offline_cli_path_renders_full_report_from_fixtures():
    report = tw.build_report(FIXTURES / "repo", 7, TODAY, offline_dir=FIXTURES)
    lines = report.splitlines()
    assert len(lines) == REPORT_LINES
    assert lines[1] == "KEV nuevos: 2 (últimos 7 días)"
    assert lines[5] == "Avisos OSV que tocan dependencias: 3"
    assert lines[9] == "Coincidencias con inventario: 2 (posible, revisar a mano)"
    assert lines[12] == "Fuentes NO leídas: ninguna"


def test_offline_report_states_osv_not_read_when_fixture_missing(tmp_path):
    (tmp_path / "kev.json").write_text(json.dumps(_load_kev_fixture()), encoding="utf-8")
    report = tw.build_report(FIXTURES / "repo", 7, TODAY, offline_dir=tmp_path)
    lines = report.splitlines()
    assert len(lines) == REPORT_LINES
    assert lines[5].endswith("fuente no leída")
    assert lines[12].startswith("Fuentes NO leídas: OSV (")
