# Pruebas externas

Registro fechado de pruebas hechas por terceros (modelos en la nube o personas) sobre Tylluan,
sin guion del equipo. Sirve para saber qué ve alguien que no conoce el proyecto.

## Reglas

- **Sin guion.** El probador descarga el proyecto de GitHub y lo prueba a su manera. Un prompt
  neutro (instalar, usar, auditar, contar lo que falla) basta; no se le dice qué encontrar.
- **Cada prueba lleva fecha, versión probada, entorno y quién la hizo** (modelo o persona).
- **El informe del probador se archiva tal cual**; las conclusiones del equipo van aparte.
- **Todo hallazgo se contrasta con un comando** antes de darlo por cierto. Una cifra o una
  referencia que el probador cita y nadie ha comprobado se marca «sin verificar».
- Nada de datos personales ni rutas de usuario en estos ficheros.

## Índice

| # | Fecha | Versión | Entorno | Quién | Entrada |
|---|-------|---------|---------|-------|---------|
| 1 | 2026 (primavera) | v0.16.0 | lectura estática | auditor externo | [2026-spring_audit_v0.16.0.md](2026-spring_audit_v0.16.0.md) |
| 2 | 2026-11 (días antes del 16) | main de la época | portátil Linux de laboratorio | equipo (pruebas internas en otro sistema) | [2026-11_linux_lab.md](2026-11_linux_lab.md) |
| 3 | 2026-11-16 | v0.17.2 | sandbox Debian 13 x86_64 | modelo en la nube (Z.ai) | [2026-11-16_sandbox_v0.17.2.md](2026-11-16_sandbox_v0.17.2.md) |

Plantilla: [TEMPLATE.md](TEMPLATE.md).
