# Tylluan — Estructura de Proyecto (Propuesta)

> Estado: PROPUESTA, no ejecutada. Tylluan ya está bien organizado (benchmarks/, docs/, scripts/ ya existen y se usan correctamente) — el hallazgo aquí es menor.

## Árbol actual (ya correcto, sin cambios)

```
tylluan/
├── README.md · LICENSE · CLAUDE.md · AGENTS.md · SPEC.md · STATUS.md · ROADMAP.md
├── AI_POLICY.md · CODE_OF_CONDUCT.md · CONTRIBUTING.md · DISCLAIMER.md · SECURITY.md · CHANGELOG.md
├── Cargo.toml · Cargo.lock · rust-toolchain.toml · dist-workspace.toml
├── tylluan.toml · tylluan.example.toml · tylluan.docker.toml
├── Dockerfile · docker-compose.yml
├── install.sh · install.ps1 · tylluan-mcp.bat · tylluan-mcp.sh
├── crates/ · guilds/ · dashboard/ · docs/ · skills/ · team/ · benchmarks/ · scripts/ · tests/ · tools/ · examples/ · assets/ · integrations/
└── briefings/ (activo, sin equivalente docs/archive/briefings/ todavía — ver hallazgo)
```

## Único hallazgo real

| Archivo actual | Problema | Destino propuesto |
|---|---|---|
| `crabs.md` (raíz) | Notas de sesión sueltas (M20, ya cerrado en otros docs), nombre no descriptivo, sin referencias en código/CI | `scratch/session-notes-m20.md` o eliminar si ya está reflejado en STATUS.md/CHANGELOG.md (verificar antes de decidir) |
| `briefings/` (raíz) | Sin carpeta de archivo para briefings cerrados | Crear `docs/archive/briefings/` con el mismo lifecycle documentado en `AGENTS.md` |

## Limpieza de disco (destructivo — requiere confirmación aparte)

| Carpeta | Tamaño | Nota |
|---|---|---|
| `target_tmp/` | 4.3 GB | Build residual, no referenciado activamente |

No se toca en este milestone — es borrado, no reorganización.

## Fuera de alcance

Todo lo demás ya sigue el patrón correcto (scripts en `scripts/`, benchmarks en `benchmarks/`, docs en `docs/`) — no hay más movimientos que proponer.

## Próximo paso

Confirmar si `crabs.md` se archiva o se elimina (verificar contra STATUS.md/CHANGELOG.md primero) → crear `docs/archive/briefings/` → commit.
