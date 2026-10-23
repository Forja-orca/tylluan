# H-19 — Cierre de 4 dudas de seguridad (espec TL T1628)

Fuentes crudas descargadas y persistidas en el scratch de la sesion (17 ficheros). Cita literal entre «»; [I]=inferencia; «NO ENCONTRADO»=sin fuente primaria leida.

## 1) Bun: que release corrige las intermedias eMudhra; si 1.3.14 la tiene
- GHSA oven-sh/bun (publicada 2026-10-23T02:02:06Z): «Bun 1.4.4 removes the root from the bundle and rejects any certificate chain that contains one of the eight compromised public keys.» https://github.com/oven-sh/bun/security/advisories/GHSA-26q8-h657-pf84
- Cambios: «Bun 1.4.4 has two changes:» + «Three emSign roots are removed and the bundle is updated to NSS 3.131.1» (commits #46159, #46300, misma GHSA).
- Release page: published_at 2026-10-22T10:11:32Z; el body no menciona eMudhra. https://bun.com/blog/release-notes/ (API tag bun-v1.4.4)
- 1.3.14: NO la tiene. Rango GHSA, JSON literal (hay espacio tras los dos puntos — sin él NO es Ctrl+F-able): «"vulnerable_version_range": "< 1.4.4"» y «"patched_versions": "1.4.4"» y la GHSA dice «Bun 1.4.3 and earlier accept certificates issued by those intermediates» (texto de la advisory, leido en la descripcion JSON). 1.3.14 es anterior a 1.4.4 → vulnerable. [I por rango]

## 2) Windows: aviso Microsoft/Mozilla del distrust de las 8 intermedias
- Aviso dedicado de Microsoft: NO ENCONTRADO en domains microsoft.com (busqueda learn+support, 3 queries).
- Mecanismo (Learn): «automatic mechanism called the CTL Updater»; el CTL de no-confiados se descarga de ctldl.windowsupdate.com/.../disallowedcertstl.cab https://learn.microsoft.com/en-us/windows-server/identity/ad-cs/certificate-trust
- Verificacion byte del CTL en vivo: INTENTO FALLIDO — el .cab es cabinet MSZIP y no lo descomprimi; `certutil -syncWithWU` no dejo ficheros en mi shell. La fecha «Microsoft bloqueo el 20-oct» solo aparece en fuentes de terceros (emudhra.we-have-a-problem.com, blog Chrome, fleetdm): [no cita primaria Microsoft].
- Mozilla si tiene aviso primario (blog 2026-10-20): «On Saturday, October 17, at 20:06 UTC, Mozilla received a report» y «blocking the affected certificates in Firefox using» [link OneCRL en la misma linea de la fuente] https://blog.mozilla.org/security/2026/10/20/protecting-firefox-users-following-the-compromise-of-emudhra-intermediate-certificates/ (via proxy de lectura; curl directo = Cloudflare block)
- eMudhra confirma el distrust: «the browser root programs have also distrusted that one ECC root» https://emudhra.com/en/post/certificate-security-incident

## 3) Windows 11 24H2 Pro: fin de soporte y paso a 25H2
- Lifecycle (Learn): «Version 24H2» retiro «10/14/2026 6:59:59 AM» https://learn.microsoft.com/en-us/lifecycle/products/windows-11-home-and-pro
- Release health: «As of October 13, 2026, Windows 11, version 24H2 Home and Pro editions have reached end of updates.» (contiene NBSP tras «As of») y «Windows 11, version 24H2 Enterprise and Education editions remain supported until October 12, 2027.» https://learn.microsoft.com/en-us/windows/release-health/status-windows-11-24H2
- Como se pasa a 25H2: KB5054156 — «enablement package,” a small, quick-to-install “master switch” that activates the Windows 11, version 25H2 features» + «update from version 24H2 to version 25H2 with a single restart» https://support.microsoft.com/en-us/servicing/os/windows/docs/2025/02/kb5054156-feature-update-to-windows-11-version-25h2-by-using-an-enablement-package

## 4) Oracle CPU octubre 2026: CVE-2026-91403 (JSSE) y JDK 21
- CVE-2026-91403 EXISTE. Oracle Security Alert (2026-October-21, Rev 1): «This Security Alert addresses CVE-2026-91403 by providing Java updates which distrusts the set of impacted intermediate CA certificates related to eMudhra.» https://www.oracle.com/security-alerts/alert-cve-2026-91403.html
- Componente JSSE: «(component: JSSE)» en la forma text del risk matrix https://www.oracle.com/security-alerts/cve-2026-91403verbose.html
- Ojo: el CPU regular (cpuoct2026.html) NO contiene «91403» (0 apariciones, 616 KB leidos; CPU Java SE afecta a «21.0.12.1»): el fix es el Security Alert del dia siguiente. [verificado grep]
- Versiones 21 afectadas y corregidas: OpenJDK advisory 2026/10/21: «The affected versions are 27.0.1, 25.0.5, 21.0.13, 17.0.21, 11.0.32.1, 8u504, and earlier.» → corregido en JDK 21.0.14 (etiquetas repo jdk21u-dev: «jdk-21.0.12.1-ga» y «jdk-21.0.14+0» existen ambas; fecha de la etiqueta via API commit no la verifique). [I: 21.0.14=fix, no confirmado por release notes leidas] https://openjdk.org/groups/vulnerability/advisories/2026-10-21

## Gate de citas (auto-verificacion)
Comando: `python - <<'EOF' ... norm(f)=strip-tags+collapse-whitespace+html.unescape; 22/22 OK contra los 17 ficheros crudos (ghsa.json, bun144.html, rel24h2.html, lifecycle.html, kb5054156.html, ora91403.html, oraverbose.html, cpuoct.html, octcpu.html, openjdk.html, mozblog2.html, emudhra.html, eminc.html, ctltrust.html, tags21.json)`. 2 FAIL initiales corregidos: (a) NBSP U+00A0 en la frase de rel24h2 → cita con \xa0 exacto; (b) cita de mozblog cortada por el enlace markdown de la fuente → split en 2 fragmentos. grep -F plano falla por NBSP: usar normalizador python que colapse \xa0.

## Verificacion del TL (2026-10-24)
Descargue la advisory de Bun (rango `< 1.4.4`), la pagina de estado de Windows 11 24H2 (frase de fin de actualizaciones y Enterprise hasta octubre de 2027), KB5054156 (paquete de habilitacion, un reinicio), el Security Alert de Oracle (distrust de las intermedias de eMudhra) y el aviso de OpenJDK (afectadas hasta 21.0.13): 5 citas literales mas la del rango de Bun. No comprobe el aviso de Mozilla, la pagina de eMudhra ni las etiquetas de jdk21u. La correccion 21.0.14 es inferencia de Hermes.
