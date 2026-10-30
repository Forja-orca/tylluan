# Informe de Implementación — Tarea AG-14: Manejo del Estado Vacío con dev_mode=false

> **Aviso:** No verificado por el Tech Lead. Diff sin commitear generado en `E:\tylluan-wt-antigravity`.

---

## 1. Resumen Ejecutivo y Objetivo

En entornos donde el kernel de Tylluan opera con la directiva estricta de seguridad `dev_mode = false` (contrato D-72 implementado por `@deep`), cualquier petición no autenticada hacia la API REST o suscripciones SSE retorna un código de estado HTTP 401 Unauthorized (`missing_token` o `invalid_token`).

Con anterioridad a esta tarea, el dashboard web reaccionaba mostrando un banner genérico rojo de *"Error de Comunicación: Unauthorized"* o dejaba paneles desiertos sin orientación al usuario sobre el origen del fallo. La tarea **AG-14** solventa esta fricción implementando una experiencia guiada y accesible:
1. Detecta respuestas de falta de token o no autorización (`unauthorized` / `missing_token`).
2. Muestra un componente dedicado y accesible (`MissingTokenBanner`) que explica la activación de la seguridad estricta, indica la localización del archivo de credenciales (`.tylluan-token`) y proporciona un botón de acción directa para abrir el diálogo de configuración (`AuthModal`).
3. Mantiene la suite de pruebas en verde (135 tests pasando, 18 suites) y certifica la compilación limpia de TypeScript y Vite.

---

## 2. Archivos Modificados y Creados

| Archivo | Estado | Descripción |
|---|---|---|
| `dashboard/src/components/ui/MissingTokenBanner.tsx` | Creado | Componente accesible con `role="region"` y `aria-label="Aviso de autenticación requerida"`. Presenta badge de seguridad activa, explicación didáctica, indicación de `.tylluan-token` y botón de apertura de `AuthModal`. |
| `dashboard/src/components/__tests__/MissingTokenBanner.test.tsx` | Creado | Suite de pruebas unitarias Vitest certificando el renderizado de la información, presencia de textos clave y emisión de eventos de clic. |
| `dashboard/src/App.tsx` | Modificado | Cableado en el contenedor principal de la vista: ante errores de tipo 401 / unauthorized / missing token, reemplaza el banner rojo genérico por `<MissingTokenBanner onOpenAuthModal={() => setAuthModalOpen(true)} />`. |
| `docs/internal/screenshots/ag14_missing_token.png` | Creado | Captura viva tomada con Chromium Headless de la interfaz en ejecución con el banner de autenticación requerida. |

---

## 3. Verificación de Calidad y Pruebas

### 3.1. Pruebas Unitarias (Vitest)
Ejecución en `E:\tylluan-wt-antigravity\dashboard`:
```bash
pnpm test --run
```
**Resultado:**
- **18 archivos de prueba pasados (18/18)**.
- **135 tests en verde (135/135)**, incluyendo:
  - `MissingTokenBanner (AG-14) > renderiza la información de seguridad activa y aviso dev_mode=false` (PASS).
  - `MissingTokenBanner (AG-14) > llama a onOpenAuthModal al hacer clic en el botón de introducir token` (PASS).
  - Cero regresiones en los 28 tests de accesibilidad estructural (`AxeAudit.test.tsx`).

### 3.2. Compilación y Chequeo de Tipos
Ejecución en `E:\tylluan-wt-antigravity\dashboard`:
```bash
pnpm build
```
**Resultado:**
- `tsc` completado sin errores de tipado.
- `vite v5.4.21 building for production...`: 1865 módulos transformados, chunks generados limpiamente en `dist/` en 24.48s.

### 3.3. Captura Viva
- Se generó la captura [ag14_missing_token.png](screenshots/ag14_missing_token.png) verificando la fidelidad visual, la coherencia de estilos con la paleta Salvia y la ausencia de desbordamientos.

---

## ## Impact

- **Seguridad y Usabilidad:** Los operadores que arranquen o interactúen con Tylluan en modo estricto (`dev_mode = false`) reciben un diagnóstico directo e inequívoco sobre la necesidad de suministrar el Bearer token, evitando confusiones con caídas del servidor o fallos de red.
- **Acoplamiento Nulo:** La lógica se integra en el flujo declarativo existente de eventos `nexus_unauthorized` y en el estado global de error de `App.tsx`, sin alterar contratos de red ni dependencias externas.
- **Invariantes:** No se han tocado dependencias en `package.json`, ni se ha modificado la iconografía de la lechuza, preservando `dashboard/public/forja-logo.png` y `dashboard/public/tylluan-logo.jpg`.
