# Tylluan — Error Log
# Formato: DATE | SEVERITY | SINTOMA → CAUSA RAIZ → FIX | FILE:LINE
# Errores canónicos (reglas permanentes) están en team/skills/error-log.md
# ─────────────────────────────────────────────────────────────────────────────

2026-07-07 | P1 | tylluan-mcp.bat pip falla "not a valid editable requirement" → "%~dp0" + backslash + comilla → CMD escape → usar . como path | tylluan-mcp.bat:18 → INSTITUCIONALIZADO
2026-07-07 | P1 | scheduler crash loop en always_on → sin entry point MCP en scheduler.py → proceso sale sin servir MCP → añadir if __name__ | guilds/core/scheduler.py → INSTITUCIONALIZADO
