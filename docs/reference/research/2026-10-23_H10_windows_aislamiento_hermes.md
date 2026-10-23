# H-10 — Primitivas nativas de Windows para aislar procesos hijo (hermes)

Asignación: TL claude-code, turno T1580 (CICLO 17:17). Detectada tarde por el watchdog; retraso pedido al TL en Coloquio.
Fuentes: exclusivamente learn.microsoft.com (9 páginas guardadas en la sesión). Cita literal Ctrl+F-able · [I]=inferencia, no citada.

| Tema | Cita literal (EN) | Página (learn.microsoft.com) | Estado |
|---|---|---|---|
| Límites de job | “A job can enforce limits such as working set size, process priority, and end-of-job time limit on each process that is associated with the job.” | …/win32/procthread/job-objects | CITADA, grep -F OK |
| Seguridad en jobs ≥Vista | “Starting with Windows Vista, security limits must be set individually for each process associated with a job object.” | …/win32/procthread/job-objects | CITADA, grep -F OK |
| UI restrictions | “The lpJobObjectInfo parameter is a pointer to a JOBOBJECT_BASIC_UI_RESTRICTIONS structure.” | …/api/jobapi2/nf-jobapi2-setinformationjobobject | CITADA, grep -F OK |
| Qué hace un token restringido | “The restricted token can have disabled security identifiers (SIDs), deleted privileges, and a list of restricting SIDs.” | …/api/securitybaseapi/nf-securitybaseapi-createrestrictedtoken | CITADA, grep -F OK |
| Hijo con token restringido | “You can use the restricted token in the CreateProcessAsUser function to create a process that has restricted access rights and privileges.” | …/api/securitybaseapi/nf-securitybaseapi-createrestrictedtoken | CITADA, grep -F OK |
| Aislamiento de escritorio | “Applications that use restricted tokens should run the restricted application on desktops other than the default desktop.” | …/api/securitybaseapi/nf-securitybaseapi-createrestrictedtoken | CITADA, grep -F OK |
| MIC: bloqueo cross-IL | “a principal with a low integrity level cannot write to an object with a medium integrity level, even if that object's DACL allows write access to the principal.” | …/secauthz/mandatory-integrity-control | CITADA, grep -F OK |
| MIC: IL del hijo | “the new process is created with the minimum of the user integrity level and the file integrity level.” | …/secauthz/mandatory-integrity-control | CITADA, grep -F OK |
| MIC: objetos sin etiqueta | “Objects that lack an integrity label are treated as medium by the operating system; this prevents low-integrity code from modifying unlabeled objects.” | …/secauthz/mandatory-integrity-control | CITADA, grep -F OK |
| AC sin empaquetar | “An unpackaged app can run in an app container, too.” | …/secauthz/appcontainer-for-legacy-applications- | CITADA, grep -F OK |
| MSIX facilita AC | “It's particularly easy to use AppContainer if you package using MSIX.” | …/secauthz/appcontainer-for-legacy-applications- | CITADA, grep -F OK |
| AC: hijo hereda token | “When an unpackaged process running in an app container calls CreateProcess , the child process typically inherits the parent's token.” | …/secauthz/appcontainer-for-legacy-applications- | CITADA, grep -F OK |
| AC: capabilities negadas por defecto | “without the network capability, an AppContainer cannot access the network, without the webcam capability it can’t access a camera.” | …/secauthz/implementing-an-appcontainer | CITADA, grep -F OK |
| AC: sensores bloqueados | “These resources are blocked by default and can be granted access as necessary.” | …/secauthz/appcontainer-isolation | CITADA, grep -F OK |
| AC vía CreateProcess | “If this attribute is set the new process will be created as an AppContainer process.” | …/api/processthreadsapi/nf-processthreadsapi-updateprocthreadattribute | CITADA, grep -F OK |
| AC perfil otro usuario | “To create a profile on behalf of another user, you must impersonate that user.” | …/api/userenv/nf-userenv-createappcontainerprofile | CITADA, grep -F OK |

## Respuesta por primitiva
- **Job Objects**: límites duros (working set, prioridad, time limit) + UI restrictions (JOBOBJECT_BASIC_UI_RESTRICTIONS). Desde Vista, límites de seguridad por proceso, no por job. Crear/asignar jobs propios: sin mención de privilegios en las páginas → [I] no requiere admin.
- **CreateRestrictedToken**: desactiva SIDs, borra privilegios, añade restricting SIDs; se aplica al hijo vía CreateProcessAsUser. Docs recomiendan escritorio no default. [I] sin admin.
- **Integrity levels (MIC)**: hijo = min(IL usuario, IL archivo) → BAJAR la IL del hijo es gratis para cualquier usuario; SUBIR exige token elevado (admin) por la regla min(). Sin etiqueta = medium.
- **AppContainer**: aislamiento por defecto-deny (red/cámaras/sensores bloqueados salvo capability); NO exige empaquetado (legacy/desktop OK, MSIX facilita); hijo hereda el token AC vía CreateProcess; perfil AC para otro usuario exige impersonación (E_ACCESSDENIED si no).

## ¿Admin o empaquetado? (respuesta directa de H-10)
- **Ninguna de las 4 primitivas exige admin ni app empaquetada** para aislar un hijo propio en sesión de usuario normal. [I] basada en ausencia de requisito de privilegios en las 9 páginas + regla min() citada.
- Excepciones citadas: subir IL del hijo por encima de la del padre (regla min) y crear perfil AC de otro usuario (impersonación).
- Sin verificar en páginas leídas (flag): SeDebugPrivilege para asignar procesos ajenos a un job; loopback exemption para red localhost en AC. Si al TL le importa, las persigo.

## Gate de citas
grep -F de las 16 citas sobre las páginas guardadas: 16/16 OK (1 con apóstifo U+2019 preservado). Ficheros .html/.txt en scratch de la sesión.

## Nota Tylluan
- Encaje kernel: lanzar procesos host con CreateProcess + PROC_THREAD_ATTRIBUTE_SECURITY_CAPABILITIES (cita AC) y perfil CreateAppContainerProfile; el hijo hereda el contenedor (cita legacy) — análogo Windows de Landlock restrict_self (H-9).
- Regla 1: único cambio = este doc, SIN commitear, en tylluan-wt-hermes (HEAD == origin/main al escribir).

## Verificacion del TL (2026-10-24)
Re-ejecutadas con curl + grep -F sobre learn.microsoft.com: AppContainer sin empaquetar ("An unpackaged app can run in an app container, too."), MIC bajo->medio, CreateRestrictedToken y Job Objects (Vista): 4/4 literales. Las conclusiones "no requiere admin" son inferencias de Hermes marcadas [I] (ausencia de requisito en las paginas leidas), NO probadas con un proceso real.
