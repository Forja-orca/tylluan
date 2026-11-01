# Guía de Interfaz de Tylluan: "Equilibrio Cálido-Profundo" (v2, 2026-11-01)

> **Estado:** Activa (reemplaza la edición v1 "vieja escuela Windows 2000/XP" del 2026-10-30 por decisión de José y cierre de diseño en Coloquio `#2404`–`#2405`).
> **Alcance:** Dashboard React (`dashboard/`) y superficies de operación multiagente (Coloquio). El instalador nativo de Windows (`docs/design/installer_mockup/`) conserva su estructura clásica de asistente de ventana fija (`560×420`).

---

## 1. Principio Rector

Tylluan evita los dos extremos que deterioran la legibilidad en sesiones largas:
1. **El extremo "plantilla genérica de IA":** fondos negro-azulados saturados con brillos neón (`box-shadow: 0 0 18px`), degradados decorativos, píldoras gigantes y texto monoespaciado en toda la interfaz.
2. **El extremo "árido sin jerarquía" (Windows 2000 estricto):** grises/beiges planos (`#ECE9D8`), bordes duros (`#ACA899`/`#808080`), esquinas bloqueadas a `1–2 px`, fuente `Tahoma` a `10–11 px` y cajas de mensaje completamente teñidas por color de agente.

El sistema **"Equilibrio Cálido-Profundo"** combina la calidez editorial y legibilidad de **Claude Desktop** en modo claro con la sobriedad nocturna de **Grok Build / Google Antigravity** en modo oscuro, manteniendo **cero dependencias externas**, **fuentes autohospedadas** y **contraste WCAG 2.2 AA verificado con cifras**.

---

## 2. Reglas Comprobables del Sistema de Diseño

### 2.1 Tipografía Autohospedada (Soberanía Local)
- **Interfaz y lectura (`font-sans`):** `'Plus Jakarta Sans', 'Segoe UI', system-ui, -apple-system, sans-serif` (servida localmente desde `dashboard/src/assets/fonts/plus-jakarta-sans-latin-*.woff2`, pesos 400 y 700).
- **Código, identificadores, turnos y telemetría (`font-mono`):** `'JetBrains Mono', Consolas, 'Courier New', monospace` (servida localmente desde `dashboard/src/assets/fonts/jetbrains-mono-latin-*.woff2`, pesos 400 y 700).
- **Cero llamadas a CDNs externos** (prohibido `fonts.googleapis.com` o `fonts.gstatic.com`).
- **Escala de lectura en Coloquio:** cuerpo de mensaje en `13 px` con interlineado `1.55` (`11.5 px` / `1.45` en modo compacto). Metadatos de cabecera y turnos en `11–12 px`.

### 2.2 Paleta Dual ("Papel Cálido" en Claro / "Observatorio Nocturno" en Oscuro)
Tres niveles nítidos de superficie (`background` → `muted` → `card`) y **doble acento con rol semántico estricto**:

| Token CSS | Tema Claro ("Papel Cálido") | Tema Oscuro ("Observatorio Nocturno") | Rol Semántico y Contraste AA |
| :--- | :--- | :--- | :--- |
| `--background` | `44 26% 95%` (`#F5F3EC`) | `224 28% 7%` (`#0D1017`) | Lienzo base reposado, sin blanco ni negro puros. |
| `--card` | `0 0% 100%` (`#FFFFFF`) | `222 24% 11%` (`#151A23`) | Superficie elevada para tarjetas, paneles y mensajes de Coloquio. |
| `--muted` | `42 20% 91%` (`#ECE9DF`) | `223 21% 14%` (`#1C212C`) | Cabeceras de panel, barras secundarias y bloques plegables. |
| `--foreground` | `220 18% 13%` (`#1B2028`) | `214 25% 93%` (`#E8EDF5`) | Texto principal (`14.4:1` en claro, `15.4:1` en oscuro sobre fondo; `16.2:1` / `13.6:1` sobre tarjeta). |
| `--muted-foreground` | `220 12% 33%` (`#4A515E`) | `215 16% 69%` (`#A2ADC0`) | Texto secundario y etiquetas (`7.1:1` en claro, `6.5:1` en oscuro). |
| `--border` | `40 12% 76%` (`#C9C5B8`) | `220 18% 22%` (`#2E3643`) | Separación estructural limpia de `1 px`. |
| `--primary` | `16 65% 38%` (`#A03D1E`) | `24 85% 60%` (`#F28740`) | **Acento primario (Terracota/Ámbar Tylluan):** acciones humanas, pestaña activa, foco de teclado (`6.3:1` sobre `#FFFFFF`, `7.3:1` sobre `#151A23`). |
| `--signal` (`cyan`) | `192 82% 28%` (`#0D6782`) | `188 86% 53%` (`#22D3EE`) | **Señal en vivo:** reservado exclusivamente a streaming SSE activo, indicador `escribiendo...` y presencia online en tiempo real (`6.4:1` claro, `9.8:1` oscuro). |

### 2.3 Radios y Elevación Moderada
- **Esquinas (`borderRadius`):** `sm: 4px`, `DEFAULT/md: 6px`, `lg: 8px`, `xl: 10px`. Se prohíben tanto los radios de `0–2 px` generalizados como las tarjetas hiper-redondeadas (`>14 px`).
- **Sombras (`boxShadow`):** sombras multicapa de baja opacidad (`sm: 0 1px 2px rgba(15,23,42,0.04)`, `DEFAULT: 0 1px 3px rgba(15,23,42,0.06), 0 1px 2px -1px rgba(15,23,42,0.04)`) para separar tarjetas del lienzo sin resplandores ni efectos neón.

### 2.4 Tarjetas de Mensaje e Identidad por Agente en Coloquio
- **Superficie limpia:** todas las tarjetas de mensaje en `ColoquioMessagesPanel.tsx` usan superficie neutra elevada (`bg-card text-card-foreground border border-border rounded-lg shadow-sm`) en lugar de teñir el fondo completo del mensaje.
- **Acento lateral (`border-l-2`):** la identidad visual de cada agente (`agent-meta.ts`) se expresa únicamente en el badge de inicial (`w-8 h-8 rounded-md`), el color accesible del nombre `@agente` y el **borde izquierdo de `2 px` (`border-l-2`)** de la tarjeta:
  - `jose`: esmeralda (`border-l-emerald-600 dark:border-l-emerald-400`)
  - `claude-code`: terracota/ámbar primario (`border-l-primary`)
  - `deep`: cian (`border-l-cyan-600 dark:border-l-cyan-400`)
  - `antigravity`: violeta (`border-l-violet-600 dark:border-l-violet-400`)
  - `buffy`: rosa profundo (`border-l-rose-600 dark:border-l-rose-400`)
  - `dsh`: índigo (`border-l-indigo-600 dark:border-l-indigo-400`)
  - `mimo`: verde azulado (`border-l-teal-600 dark:border-l-teal-400`)
  - `qwen`: azul (`border-l-blue-600 dark:border-l-blue-400`)
  - `opencode`: ámbar (`border-l-amber-600 dark:border-l-amber-400`)

### 2.5 Accesibilidad (WCAG 2.2 AA) y Logo Oficial
- **Contraste:** mínimo `4.5:1` medido para todo texto normal en ambos temas (`light` y `dark`).
- **Teclado y foco:** `:focus-visible` con contorno nítido de `2 px solid hsl(var(--primary))` y `outline-offset: 1px`. Todos los controles interactivos conservan navegación por `Tab`, `Enter` y cierre contextual con `Escape`.
- **Logo intocable:** `dashboard/public/tylluan-logo.jpg` (`TylluanLogo.tsx`), búho azul original sobre contenedor blanco con borde sutil. Nunca sustituir por SVGs sintéticos ni invertir con filtros CSS.
- **Veracidad del texto:** español llano, sin datos simulados ni afirmaciones no verificadas.
