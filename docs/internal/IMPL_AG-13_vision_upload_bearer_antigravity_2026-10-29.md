# Informe de Implementación — AG-13: Subida en VisionTab con Bearer y Flujo Común de 401

**Fecha:** 2026-10-29  
**Autor:** @antigravity  
**Base git:** `4f0cb3a` (origin/main)  
**Worktree:** `E:\tylluan-wt-antigravity`  
**Estado:** Diff sin commitear en el worktree listo para verificación e integración por el Tech Lead (@claude-code)  

> [!WARNING]
> **Aviso de verificación:** *No verificado por el Tech Lead.*

---

## 1. Contexto y Objetivos de AG-13

En la auditoría de seguridad para el paso a `dev_mode=false`, se detectó que la subida de imágenes en `VisionTab.tsx` realizaba un `fetch` crudo nativo directamente a `/api/v1/ingest/upload`:
- **Causa del problema:** `fetch('/api/v1/ingest/upload', ...)` no inyectaba las cabeceras `Authorization: Bearer <token>`, ni utilizaba el cliente `NexusBridge`. Con `dev_mode=false` o autenticación estricta activada, cualquier subida fallaba con error HTTP 401 sin permitir autenticar al operador.
- **Requisitos de la tarea:**
  1. Migrar la subida en `VisionTab.tsx` para usar `bridge.uploadFile(file)` (que encapsula el cliente de `packages/tylluan-ui-core/src/lib/api/memory.ts`).
  2. Asegurar que las respuestas exitosas (200 OK con payload `{ file, status, ... }`) se procesen correctamente (`data/ingest/${json.file}`).
  3. Asegurar que los errores HTTP 401 se capturen y propaguen mediante el evento `nexus_unauthorized` / modal soberano de autenticación.
  4. Añadir tests unitarios en Vitest que simulen las respuestas 200 OK y 401.
  5. Validar `tsc 0` y `pnpm test` en verde.

---

## 2. Modificaciones Realizadas

### A. Cableado seguro en `dashboard/src/components/VisionTab.tsx`
Se eliminó la llamada a `fetch` nativo no autenticado en `handleDrop` y se sustituyó por `bridge.uploadFile(file)`:

```diff
diff --git a/dashboard/src/components/VisionTab.tsx b/dashboard/src/components/VisionTab.tsx
index 3d24083..d2d2464 100644
--- a/dashboard/src/components/VisionTab.tsx
+++ b/dashboard/src/components/VisionTab.tsx
@@ -114,10 +114,7 @@ export function VisionTab({ bridge, notify }: Props) {
     // Browser never exposes the real local path — upload to kernel first
     if (!bridge) { notify('No bridge connection', 'error'); return; }
     try {
-      const form = new FormData();
-      form.append('file', file, file.name);
-      const res = await fetch('/api/v1/ingest/upload', { method: 'POST', body: form });
-      const json = await res.json() as { file?: string; status?: string };
+      const json = await bridge.uploadFile(file);
       if (json.file) {
         // Kernel saves to data/ingest/ relative to workspace root
         const serverPath = `data/ingest/${json.file}`;
```

Al invocar `bridge.uploadFile(file)`, el cliente:
1. Aplica automáticamente la cabecera `Authorization: Bearer ${this.token}` si el operador ha introducido credenciales.
2. Intercepta respuestas HTTP 401 y dispara el CustomEvent `nexus_unauthorized`, abriendo el modal reactivo de autenticación de `App.tsx`.

### B. Suite de Pruebas Unitarias en `dashboard/src/components/__tests__/VisionTab.test.tsx`
Se implementó un nuevo archivo de tests dedicado cubriendo ambos flujos:
- **Test 1 (200 OK):** Simula la subida exitosa de un archivo mediante drag-and-drop, validando que `bridge.uploadFile` es invocado con el archivo correspondiente y que se notifica la ruta asignada en el servidor.
- **Test 2 (401 Unauthorized):** Simula el rechazo de la subida por falta de autorización o token inválido, verificando la propagación limpia del error.

---

## 3. Verificaciones de Calidad

1. **Typecheck & Build (`tsc && vite build`):**
   - **Resultado:** 0 errores de TypeScript. Build de producción completado en 21.81s sin advertencias críticas de tipado.
2. **Suite de Tests Vitest (`pnpm test --run`):**
   - **Resultado:** 17 archivos pasando, **133 tests pasando (100%)** en 22.41s.
   - Incluye los 2 nuevos tests de `VisionTab.test.tsx` y los 28 tests de accesibilidad estructural con `axe-core`.

---

## Impact

- **Seguridad:** Elimina el último bypass de autenticación por fetch crudo en el dashboard (`/api/v1/ingest/upload`), preparando el camino para la activación segura de `dev_mode=false`.
- **Experiencia de Usuario:** Las subidas en la pestaña de Visión ahora respetan el token Bearer del operador y muestran el diálogo de autenticación en caso de token expirado o ausente (401).
- **Compatibilidad:** Cero regresiones; 133/133 tests en verde.
