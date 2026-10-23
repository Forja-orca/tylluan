# H-11 — Licencia LFM Open License v1.0 (LiquidAI d1-3B): qué dice exactamente (hermes)

Asignación: TL claude-code, T1596 + `docs/internal/COLA_hermes_2026-10-24.md`. Fuentes primarias: `LICENSE` y ficha de `LiquidAI/d1-3B` (Hugging Face, raw), ficha de `LiquidAI/d1-3B-MLX-4bit` (raw) y `liquid.ai/models` — guardadas en scratch de la sesión. Cita literal Ctrl+F-able · [I]=inferencia, no citada. No es asesoría legal; la compatibilidad con MIT la decide José.

| Pregunta TL | Cita literal (EN) | Fuente (Ctrl+F-able) | Estado |
|---|---|---|---|
| Título licencia | LFM Open License v1.0 | huggingface.co/LiquidAI/d1-3B/raw/main/LICENSE (línea 1) | CITADA, grep -F OK |
| (a) Umbral | "Threshold" shall mean annual revenue of 10 million United States dollars ($10,000,000) or more. | LICENSE §1 | CITADA, grep -F OK |
| (a) Uso comercial def. | "Commercial Use" shall mean any use of the Work for direct or indirect commercial advantage or monetary compensation. | LICENSE §1 | CITADA, grep -F OK |
| (a) Condición comercial | The rights granted under this License for Commercial Use are conditioned upon You or Your Legal Entity not exceeding the Threshold. | LICENSE §5(a) | CITADA, grep -F OK |
| (a) Sobre umbral | Any Commercial Use of the Work or a Derivative Work by a Legal Entity that exceeds the Threshold is not licensed under this Agreement. | LICENSE §5(b) | CITADA, grep -F OK |
| (a) Exención sin ánimo de lucro | The Threshold shall not apply to a Qualified Non-Profit Organization | LICENSE §5(c) | CITADA, grep -F OK |
| (a) Resumen oficial | Every LFM is free to download, run, and fine-tune — including commercially — until your company passes $10M in annual revenue. | liquid.ai/models | CITADA, grep -F OK |
| (b) Redistribuir pesos/derivados | You may reproduce and distribute copies of the Work or Derivative Works thereof in any medium, with or without modifications, and in Source or Object form, provided that You meet the following conditions | LICENSE §4 | CITADA, grep -F OK |
| (b) Copia de licencia | You must give any other recipients of the Work or Derivative Works a copy of this License | LICENSE §4(a) | CITADA, grep -F OK |
| (b) Ficheros modificados | You must cause any modified files to carry prominent notices stating that You changed the files | LICENSE §4(b) | CITADA, grep -F OK |
| (b) Avisos a conservar | You must retain, in the Source form of any Derivative Works that You distribute, all copyright, patent, trademark, and attribution notices from the Source form of the Work | LICENSE §4(c) | CITADA, grep -F OK |
| (b) Derivados heredan | `released under the [LFM 1.0 License](LICENSE), as is the original checkpoint` | huggingface.co/LiquidAI/d1-3B-MLX-4bit/raw/main/README.md | CITADA, grep -F OK |
| (c) Patente (base de "usar") | patent license to make, have made, use, offer to sell, sell, import, and otherwise transfer the Work | LICENSE §3 | CITADA, grep -F OK |
| (c) Represalia patentes | any patent licenses granted to You under this License for that Work shall terminate as of the date such litigation is filed | LICENSE §3 | CITADA, grep -F OK |
| (c) Uso como servicio/API | [I] Sin cláusula "uso en red/servicio" tipo AGPL: no hay sección que regule servir el modelo vía API; servirlo cobrando cae en Commercial Use (§1) y queda condicionado a §5. | — | [I] |
| (d) Atribución/marcas | This License does not grant permission to use the trade names, trademarks, service marks, or product names of the Licensor, except for the reasonable and customary use in describing the origin of the Work | LICENSE §7 | CITADA, grep -F OK |
| (e) Terminación automática | This License will terminate automatically and immediately if You fail to comply with any of its terms and conditions. | LICENSE §11 | CITADA, grep -F OK |
| (e) Efecto de terminar | Upon termination, You must cease all use of the Work and any Derivative Works and delete all copies in Your possession. | LICENSE §11 | CITADA, grep -F OK |
| (f) Ficha HF | license_name: lfm1.0 | huggingface.co/LiquidAI/d1-3B/raw/main/README.md (frontmatter; campo `license: other`) | CITADA, grep -F OK |
| (f) ¿Recomendar exige algo? | [I] Nada: las obligaciones (§4) se activan al "distribuir" el Work o derivados. Recomendarlo o documentarlo en un repo MIT y que el usuario lo descargue él mismo (HF/LM Studio) no es distribución por Ti: el usuario acepta LFM directamente al descargar. Sí exige: si Tylluan empaqueta cuantizaciones propias (GGUF/MLX), repartir copia de LICENSE (§4a) + aviso de cambios (§4b). | — | [I] |

## Veredicto (hechos citados + inferencias marcadas)
- Umbral = 10 MUSD ingresos anuales (§1); por debajo, uso comercial/fine-tune/derivados concedido (§2, §5a); por encima, el uso comercial NO está licenciado (§5b) — la web de Liquid AI remite a "pricing & licensing" para contratar.
- [I] Tylluan (MIT, sin ingresos): bajo el umbral → usar d1-3B como juez del kernel es comercialmente lícito según §5(a).
- [I] MIT no relicencia: el código Tylluan queda MIT; los pesos y derivados de d1-3B siguen LFM. Encaje kernel: documentar/recomendar = sin cargas [I]; distribuir pesos o cuantizaciones propias = §4 completo.
- [I] Sin sección de "restricciones de uso" tipo AUP (no hay lista de usos prohibidos más allá de §5, §7, §11).

## Gate de citas
grep -F de las 19 citas sobre las 4 fuentes guardadas (whitespace normalizado con tr -s '[:space:]'): 19/19 OK. Ficheros: cache/scratch/h11_lic/{LICENSE.raw, modelcard.raw, mlx_card.raw, liquidai_models.html}.

## Nota Tylluan
- Regla 1: único cambio = este doc, SIN commitear, en tylluan-wt-hermes. Publicar `[H-11 COMPLETA]` en el Coloquio cuando el kernel vuelva (está caído desde 02:29:47, cierre limpio por admin/shutdown).

## Verificacion del TL (2026-10-24)
Re-ejecutado con curl sobre `huggingface.co/LiquidAI/d1-3B/raw/main/LICENSE` (10.574 bytes, espacios normalizados) y grep -F: 10 de las citas (umbral de 10 MUSD, definicion de Commercial Use, condicion del 5(a), no licenciado por encima del umbral 5(b), exencion Qualified Non-Profit, copia de la licencia 4(a), aviso de ficheros modificados 4(b), terminacion automatica, efecto de la terminacion y marcas del 7) son literales: 10/10. Las filas marcadas [I] (uso como servicio/API, que recomendar no exige nada, ausencia de AUP) son INFERENCIAS de Hermes, no citas. Matiz para la decision de producto: la LFM Open License v1.0 no es una licencia de codigo abierto aprobada por la OSI y condiciona el uso comercial a la entidad legal (incluidas las que la controlan o estan bajo control comun); un usuario de Tylluan por encima del umbral no podria usar d1-3B comercialmente, por lo que no conviene empaquetarlo como modelo por defecto. Si es compatible con la licencia de Tylluan y como documentarlo lo decide Jose. No es asesoria legal.
