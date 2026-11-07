#!/usr/bin/env python3
"""scripts/check_cfg_deadcode.py — gate anti dead_code cross-platform (T-HER-3 / TL T2944).

Clase de bug (push 271 tumbo CI en 6 jobs por esto): una `fn` de crate lib
definida SIN guarda cfg cuyo unico uso queda dentro de regiones imposibles en
Linux non-test (`#[cfg(target_os = "windows")]`, `#[cfg(test)]`). En Windows se
compila y usa; en Linux/ARM64 el compilador la ve sin uso -> dead_code -> con
`-D warnings` el job se pone rojo. Local en Windows no lo pilla NUNCA: por eso
hace falta este gate estatico.

Criterio de aceptacion literal del TL (T2944):
  - 0 avisos sobre d00c476b + fix (= 6daa3906)
  - exactamente 1 aviso (resumen_sonda) sobre d00c476b sin el fix
  - `fn new` de tunnel/mod.rs no puede salir nunca

Diseno exigido y trampas evitadas (intentos 1 y 2 de Deep, T2944):
  * nivel de crate entero: las apariciones se buscan en TODO el crate;
  * solo `fn` a nivel superior (columna 0) con visibilidad privada o
    pub(crate): los metodos de impl (TunnelManager::new, el falso positivo de
    Deep) nunca son candidatos por construccion;
  * lista de exclusiones por nombre AUN ASI;
  * doc-comments y comentarios EXCLUIDOS de las apariciones (trampa del intento
    2: el nombre citado en la doc de la propia funcion contaba como uso fuera);
  * predicados cfg evaluados bajo Linux non-test; atomos desconocidos/feature()
    = vivos (conservador: prefiere no avisar a avisar de mas).

No verificado (acotaciones declaradas en el informe):
  - funciones con 0 apariciones en el crate NO se avisan (esa variante la pilla
    el propio compilador dead_code sin ambiguedad de macros/constructores);
  - items anidados con cfg propio DENTRO de un bloque top-level no se refinan:
    el uso se clasifica por el item top-level (mod/fn) que lo contiene;
  - `feature(...)` y atomos exoticos = vivos (sub-notifica en esos casos);
  - no sustituye al job portability-check real de CI; es filtro previo barato.

Uso: python scripts/check_cfg_deadcode.py [ruta-repo]   (rc=1 si hay avisos)
"""
import os, re, sys

EXCLUDE = {"new", "default", "from", "into", "try_from", "try_into", "drop", "main"}

RE_FN_TL = re.compile(r'^(?:(pub\([a-z, ]*\)|)\s+)?(?:(?:const|async|unsafe)\s+)*fn\s+([A-Za-z_][A-Za-z0-9_]*)')
RE_ITEM_TL = re.compile(r'^(?:pub(?:\([a-z, ]*\))?\s+)?(?:(?:const|async|unsafe|static)\s+)*(fn|impl|mod|struct|enum|trait|union|type|macro_rules!)\b')
TOK = re.compile(r'[A-Za-z_][A-Za-z0-9_]+')

def split_code(text):
    """lineas de codigo puro: comentarios y literales -> espacios; se conservan
    saltos de linea, indentacion y llaves."""
    out, i, n, buf = [], 0, len(text), []
    def flush():
        out.append("".join(buf)); buf.clear()
    while i < n:
        c = text[i]
        if c == "\n":
            flush(); out.append("\n"); i += 1
        elif text.startswith("//", i):
            j = text.find("\n", i); j = n if j < 0 else j
            flush(); buf.extend([" "] * (j - i)); i = j
        elif text.startswith("/*", i):
            depth, j = 1, i + 2
            while j < n and depth:
                if text.startswith("/*", j): depth += 1; j += 2
                elif text.startswith("*/", j): depth -= 1; j += 2
                else: j += 1
            seg = text[i:j]; i = j
            flush(); buf.extend(" " if ch != "\n" else "\n" for ch in seg)
        elif c == 'r' and re.match(r'r#*"', text[i:i+5]):
            m = re.match(r'r(#*)"', text[i:])
            hashes = m.group(1)
            end = text.find('"' + hashes, i + 1); end = n if end < 0 else end + 1 + len(hashes)
            seg = text[i:end]; i = end
            flush(); buf.extend(" " if ch != "\n" else "\n" for ch in seg)
        elif c == '"':
            j = i + 1
            while j < n and text[j] != '"':
                j += 2 if text[j] == "\\" else 1
            seg = text[i:min(j + 1, n)]; i = min(j + 1, n)
            flush(); buf.extend(" " if ch != "\n" else "\n" for ch in seg)
        elif c == "'":
            m = re.match(r"'(\\.|[^'\\])'", text[i:i+4])
            if m:
                seg = m.group(0); i += len(seg)
                flush(); buf.extend([" "] * len(seg))
            else:
                buf.append(c); i += 1
        else:
            buf.append(c); i += 1
    flush()
    # join+split final: los segmentos multi-linea (comentarios de bloque, strings
    # con saltos) quedan alineados 1:1 con raw.split("\n") — antes se colapsaban
    # y desalineaban numero de linea/atributos (causa raiz del 0 falsos)
    return "".join(out).split("\n")

def split_top(s):
    parts, depth, cur = [], 0, []
    for ch in s:
        if ch == "(": depth += 1
        elif ch == ")": depth -= 1
        if ch == "," and depth == 0: parts.append("".join(cur)); cur = []
        else: cur.append(ch)
    if "".join(cur).strip(): parts.append("".join(cur))
    return parts

def cfg_alive(expr):
    """True: la region puede existir en Linux non-test. False: imposible."""
    expr = expr.strip()
    m = re.match(r'^any\((.*)\)$', expr, re.S)
    if m: return any(cfg_alive(p) for p in split_top(m.group(1)))
    m = re.match(r'^all\((.*)\)$', expr, re.S)
    if m: return all(cfg_alive(p) for p in split_top(m.group(1)))
    m = re.match(r'^not\((.*)\)$', expr, re.S)
    if m: return not cfg_alive(m.group(1))
    e = expr.replace(" ", "").replace("\n", "")
    if e == "windows": return False
    if e in ("unix", "linux", 'target_family="unix"'): return True
    m = re.match(r'target_os="([a-z0-9_]+)"', e) or re.match(r'target_family="([a-z0-9_]+)"', e)
    if m: return m.group(1) in ("linux", "android", "macos", "unix", "ios")
    if e == "test": return False
    return True  # feature(...), docs, desconocidos: conservador

def attrs_cfg(attr_lines):
    joined = "\n".join(attr_lines)
    cfgs = re.findall(r'#\[cfg\(((?:[^\[\]]|\[[^\]]*\])*)\)\]', joined)
    has_other = bool(re.search(r'#\[\s*(?!cfg\s*\()[a-z_]', joined))
    if has_other and cfgs: return "OTHER"
    if has_other: return "OTHER"
    if len(cfgs) == 1: return cfgs[0]
    if len(cfgs) > 1: return "all(" + ",".join(cfgs) + ")"
    return None

def parse_items(code, raw_lines):
    """items a nivel superior: (kind, name, start, end, cfg|'OTHER'|None).
    1-based; end = linea de cierre del cuerpo (o start si `mod x;` sin cuerpo).
    Los ATRIBUTOS se leen de raw_lines: split_code borra literales de string y
    dentro de `#[cfg(target_os = "windows")]` eso dejaria el predicado ileible
    (medido: fue la causa del 0 avisos falsos sobre d00c476b)."""
    items, n, i, pend = [], len(code), 0, []
    while i < n:
        ln = code[i]
        if ln.startswith("#["):
            bal = ln.count("[") - ln.count("]")
            blk = [raw_lines[i] if i < len(raw_lines) else ln]; j = i
            while bal > 0 and j + 1 < n:
                j += 1
                blk.append(raw_lines[j] if j < len(raw_lines) else code[j])
                bal += code[j].count("[") - code[j].count("]")
            pend.extend(blk); i = j + 1; continue
        if ln and ln[0] != " ":
            m = RE_ITEM_TL.match(ln)
            if m:
                kind = m.group(1)
                name = ""
                mf = RE_FN_TL.match(ln)
                if mf: name = mf.group(2)
                else:
                    mm = (re.search(r'\b(?:mod|struct|enum|trait|union|type)\s+([A-Za-z_][A-Za-z0-9_]*)', ln)
                          or re.search(r'impl(?:<[^>]*>)?\s+(?:for\s+)?([A-Za-z_][A-Za-z0-9_]*)', ln)
                          or re.search(r'macro_rules!\s+([A-Za-z_][A-Za-z0-9_]*)', ln))
                    if mm: name = mm.group(1)
                cfg = attrs_cfg(pend)
                start, end = i + 1, i + 1
                depth, seen, p = 0, False, i
                while p < n:
                    for ch in code[p]:
                        if ch == "{": depth += 1; seen = True
                        elif ch == "}": depth -= 1
                    if seen and depth <= 0: end = p + 1; break
                    p += 1
                items.append((kind, name, start, end, cfg))
            elif ln.strip():
                pend = []
        i += 1
    return items

def analyze_repo(root):
    crates_dir = os.path.join(root, "crates")
    if not os.path.isdir(crates_dir):
        print("CFG-DEADCODE: no hay crates/ en", root); return []
    findings = []
    for cname in sorted(os.listdir(crates_dir)):
        src = os.path.join(crates_dir, cname, "src")
        if not os.path.isdir(src): continue
        files = {}
        for dirpath, _, names in os.walk(src):
            for nm in names:
                if not nm.endswith(".rs"): continue
                p = os.path.join(dirpath, nm)
                raw = open(p, encoding="utf-8", errors="replace").read()
                code = split_code(raw)
                files[p] = {"code": code, "items": parse_items(code, raw.split("\n"))}
        index = {}
        for p, f in files.items():
            for li, line in enumerate(f["code"], 1):
                for tok in TOK.findall(line):
                    index.setdefault(tok, []).append((p, li))
        mod_decls = {}
        for p, f in files.items():
            for kind, nm, s, e, cfg in f["items"]:
                if kind == "mod" and nm:
                    mod_decls.setdefault(nm, []).append(cfg)
        def file_alive(p):
            stem = os.path.basename(p)[:-3]
            decls = mod_decls.get(stem)
            if decls and all(c and c != "OTHER" and not cfg_alive(c) for c in decls):
                return False
            return True
        cands = []
        for p, f in files.items():
            if not file_alive(p): continue
            for kind, nm, s, e, cfg in f["items"]:
                if kind != "fn" or not nm or nm in EXCLUDE: continue
                decl = f["code"][s - 1]
                if decl[0] == " ": continue  # solo top-level: la columna 0 NO puede empezar con espacio
                if not (decl.startswith("fn") or decl.startswith("pub(")): continue
                if cfg is not None: continue
                cands.append((p, nm, s, e))
        for p, nm, s, e in cands:
            occ = index.get(nm, [])
            total_out, gated, alive = 0, 0, False
            for q, li in occ:
                if q == p and s <= li <= e: continue
                total_out += 1
                g = files[q]
                dead = False
                for kind2, nm2, s2, e2, cfg2 in g["items"]:
                    if kind2 in ("mod", "fn") and s2 <= li <= e2:
                        if cfg2 and cfg2 != "OTHER" and not cfg_alive(cfg2):
                            dead = True; break
                if not dead and not file_alive(q):
                    dead = True
                if dead: gated += 1
                else:
                    alive = True; break
            if total_out >= 1 and not alive and gated == total_out:
                findings.append((os.path.relpath(p, root).replace(os.sep, "/"), s, nm))
    return findings

def main():
    root = sys.argv[1] if len(sys.argv) > 1 else "."
    res = analyze_repo(root)
    for rel, ln, nm in sorted(res):
        print("CFG-DEADCODE %s:%d fn %s -- definida sin guarda; SOLO usada en regiones imposibles en Linux non-test" % (rel, ln, nm))
    if res:
        print("CFG-DEADCODE: %d aviso(s)" % len(res)); return 1
    print("CFG-DEADCODE: 0 avisos"); return 0

if __name__ == "__main__":
    sys.exit(main())
