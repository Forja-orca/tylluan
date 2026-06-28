"""
TylluanNexus Knowledge Guild — Automatic knowledge graph triple extraction.

This guild provides a tool to extract subject-predicate-object triples
from text for populating the SilvaDB knowledge graph.

Requires: pip install gliner
"""

import importlib.util
GLINER_AVAILABLE = importlib.util.find_spec("gliner") is not None

import logging
import re
import json
from typing import List, Dict, Any, Optional
from mcp.server.fastmcp import FastMCP

mcp = FastMCP("tylluan-knowledge")

# Module-level model (lazy loaded on first call)
# False = load was attempted and failed (don't retry)
_model = None
_model_load_failed = False

def get_model():
    """Lazy-load GLiNER model. Returns None on failure without retrying."""
    global _model, _model_load_failed
    if _model_load_failed:
        return None
    if _model is None and GLINER_AVAILABLE:
        try:
            from gliner import GLiNER
            _model = GLiNER.from_pretrained("fastino/gliner2-base-v1")
        except Exception as e:
            _model_load_failed = True
            logging.warning("Failed to load GLiNER model (regex fallback active): %s", e)
            return None
    return _model

ENTITY_LABELS = [
    "person", "software", "tool", "guild", "concept",
    "system", "file", "directory", "language", "framework",
    "error", "process"
]

# Patterns for rule-based extraction
# CamelCase, ALL_CAPS acronyms (≥2 chars), Title Case phrases
_ENTITY_REGEX = re.compile(
    r'\b('
    r'[A-Z][a-z]{3,}(?:\s+[A-Z][a-z]{2,})*'   # Title Case phrases (try first: 'Claude Desktop')
    r'|[A-Z][a-zA-Z0-9]{2,}(?:[A-Z][a-z]+)*'   # CamelCase: TylluanNexus, SilvaDB
    r'|[A-Z]{2,9}'                               # Acronyms: MCP, API, HTTP
    r')\b'
)
# Stop-words that are not entities even if capitalized
_STOPWORDS = frozenset({
    "The", "This", "That", "These", "Those", "When", "Where", "Which",
    "What", "How", "Why", "And", "But", "For", "With", "From", "Into",
    "Then", "Also", "Both", "Each", "Some", "Such", "Most", "More",
    "Use", "Used", "Using", "None", "True", "False", "Error",
})

# Common domain nouns treated as implicit entities even when lowercase.
# These appear frequently in technical/systems text as subjects or objects.
_DOMAIN_NOUNS = frozenset({
    "kernel", "server", "client", "system", "process", "service", "daemon",
    "module", "plugin", "extension", "library", "package", "binary",
    "container", "image", "network", "protocol", "transport", "endpoint",
    "route", "middleware", "handler", "controller", "manager", "registry",
    "database", "storage", "memory", "cache", "queue", "stream",
    "tool", "guild", "agent", "worker", "node", "cluster",
    "dashboard", "ui", "api", "sdk", "cli", "gui",
    "config", "setting", "option", "flag", "parameter", "argument",
    "function", "method", "class", "struct", "trait", "interface",
    "file", "directory", "path", "link", "socket", "pipe",
    "request", "response", "message", "event", "signal", "hook",
    "task", "job", "thread", "fiber", "coroutine", "promise",
    "language", "compiler", "interpreter", "runtime", "vm",
    "model", "engine", "pipeline", "workflow", "pipeline",
    "code", "script", "program", "application", "app",
})

# ─── Verb lexicon for SVO extraction ─────────────────────────────────────────
# Known verbs in English and Spanish for predicate detection.
# normalize_verb() maps these to canonical predicates.
_KNOWN_VERBS = frozenset({
    # be
    "is", "are", "was", "were", "be", "been", "being",
    # have
    "has", "have", "had",
    # action verbs
    "uses", "use", "used", "using",
    "provides", "provide", "provided", "providing",
    "contains", "contain", "contained", "containing",
    "includes", "include", "included", "including",
    "built", "builds", "build", "building",
    "creates", "create", "created", "creating",
    "runs", "run", "ran", "running",
    "supports", "support", "supported", "supporting",
    "enables", "enable", "enabled", "enabling",
    "manages", "manage", "managed", "managing",
    "processes", "process", "processed", "processing",
    "generates", "generate", "generated", "generating",
    "controls", "control", "controlled", "controlling",
    "connects", "connect", "connected", "connecting",
    "writes", "write", "wrote", "written", "writing",
    "reads", "read", "reading",
    "calls", "call", "called", "calling",
    "returns", "return", "returned", "returning",
    "takes", "take", "took", "taken", "taking",
    "sends", "send", "sent", "sending",
    "receives", "receive", "received", "receiving",
    "means", "mean", "meant", "meaning",
    "represents", "represent", "represented",
    "defines", "define", "defined", "defining",
    "implements", "implement", "implemented",
    "extends", "extend", "extended", "extending",
    "inherits", "inherit", "inherited",
    "depends", "depend", "depended", "depending",
    "belongs", "belong", "belonged",
    "produces", "produce", "produced", "producing",
    "converts", "convert", "converted", "converting",
    "stores", "store", "stored", "storing",
    "loads", "load", "loaded", "loading",
    "executes", "execute", "executed", "executing",
    "wraps", "wrap", "wrapped", "wrapping",
    "queries", "query", "queried", "querying",
    "validates", "validate", "validated",
    "transforms", "transform", "transformed",
    "maps", "map", "mapped", "mapping",
    "handles", "handle", "handled", "handling",
    "serves", "serve", "served", "serving",
    "offers", "offer", "offered", "offering",
    "speaks", "speak", "spoke",
    "talks", "talk", "talked",
    "works", "work", "worked", "working",
    "fits", "fit", "fitted",
    "matches", "match", "matched", "matching",
    "requires", "require", "required", "requiring",
    "needs", "need", "needed",
    "allows", "allow", "allowed", "allowing",
    "lets", "let",
    "makes", "make", "made", "making",
    "runs", "run", "ran", "running",
    "starts", "start", "started", "starting",
    "stops", "stop", "stopped", "stopping",
    "opens", "open", "opened", "opening",
    "closes", "close", "closed", "closing",
    "lists", "list", "listed", "listing",
    "shows", "show", "showed", "shown", "showing",
    "displays", "display", "displayed",
    "renders", "render", "rendered", "rendering",
    "draws", "draw", "drew", "drawn",
    "fetches", "fetch", "fetched", "fetching",
    "pushes", "push", "pushed", "pushing",
    "pulls", "pull", "pulled", "pulling",
    "merges", "merge", "merged", "merging",
    "splits", "split", "splitting",
    "joins", "join", "joined", "joining",
    "logs", "log", "logged", "logging",
    "prints", "print", "printed", "printing",
    "tests", "test", "tested", "testing",
    "checks", "check", "checked", "checking",
    "verifies", "verify", "verified",
    "compiles", "compile", "compiled", "compiling",
    "builds", "build", "building",
    "deploys", "deploy", "deployed", "deploying",
    "installs", "install", "installed", "installing",
    "configures", "configure", "configured", "configuring",
    "updates", "update", "updated", "updating",
    "accepts", "accept", "accepted", "accepting",
    "rejects", "reject", "rejected", "rejecting",
    "routes", "route", "routed", "routing",
    "deploys", "deploy", "deployed", "deploying",
    "hosts", "host", "hosted", "hosting",
    "integrates", "integrate", "integrated",
    "optimizes", "optimize", "optimized",
    "analyses", "analyse", "analyze", "analyzed", "analyzing",
    "extracts", "extract", "extracted", "extracting",
    "detects", "detect", "detected", "detecting",
    "embeds", "embed", "embedded", "embedding",
    "trains", "train", "trained", "training",
    "learns", "learn", "learned", "learning",
    # Spanish
    "es", "son", "era", "eran",
    "tiene", "tienen", "tener",
    "usa", "usan", "usado", "utiliza", "utilizan",
    "proporciona", "proporcionan",
    "contiene", "contienen",
    "incluye", "incluyen",
    "construido", "construye", "construir",
    "crea", "crean", "creado", "crear",
    "ejecuta", "ejecutan", "ejecutar",
    "soporta", "soportan",
    "significa", "significan",
    "representa", "representan",
    "define", "definen",
    "implementa", "implementan",
    "depende", "dependen",
    "pertenece", "pertenecen",
    "produce", "producen",
    "convierte", "convierten",
    "almacena", "almacenan",
    "carga", "cargar", "cargado",
    "procesa", "procesan", "procesar",
    "genera", "generan", "generar",
    "gestiona", "gestionan",
    "analiza", "analizan",
    "extrae", "extraen",
    "detecta", "detectan",
    "entrena", "entrenan",
})

_VERB_MAP = {
    # English copula
    "is": "is_a", "are": "is_a", "was": "is_a", "were": "is_a", "be": "is_a",
    # English have
    "has": "has_feature", "have": "has_feature", "had": "has_feature",
    # English actions
    "uses": "uses", "use": "uses", "used": "uses", "using": "uses",
    "provides": "provides", "provide": "provides", "provided": "provides", "providing": "provides",
    "contains": "contains", "contain": "contains", "contained": "contains", "containing": "contains",
    "includes": "includes", "include": "includes", "included": "includes", "including": "includes",
    "built": "is_built_with", "builds": "is_built_with", "build": "is_built_with", "building": "is_built_with",
    "creates": "creates", "create": "creates", "created": "created_by", "creating": "creates",
    "runs": "runs_on", "run": "runs_on", "ran": "runs_on", "running": "runs_on",
    "supports": "supports", "support": "supports", "supported": "supports", "supporting": "supports",
    "enables": "enables", "enable": "enables", "enabled": "enables", "enabling": "enables",
    "manages": "manages", "manage": "manages", "managed": "manages", "managing": "manages",
    "processes": "processes", "process": "processes", "processed": "processes", "processing": "processes",
    "generates": "generates", "generate": "generates", "generated": "generates", "generating": "generates",
    "controls": "controls", "control": "controls", "controlled": "controls", "controlling": "controls",
    "connects": "connects_to", "connect": "connects_to", "connected": "connected_to", "connecting": "connects_to",
    "writes": "writes_to", "write": "writes_to", "wrote": "writes_to", "writing": "writes_to",
    "reads": "reads_from", "read": "reads_from", "reading": "reads_from",
    "calls": "calls", "call": "calls", "called": "called", "calling": "calls",
    "returns": "returns", "return": "returns", "returned": "returns", "returning": "returns",
    "sends": "sends_to", "send": "sends_to", "sent": "sends_to", "sending": "sends_to",
    "receives": "receives_from", "receive": "receives_from", "received": "receives_from", "receiving": "receives_from",
    "means": "means", "mean": "means", "meant": "means", "meaning": "means",
    "represents": "represents", "represent": "represents", "represented": "represents",
    "defines": "defines", "define": "defines", "defined": "defines", "defining": "defines",
    "implements": "implements", "implement": "implements", "implemented": "implements",
    "extends": "extends", "extend": "extends", "extended": "extends", "extending": "extends",
    "inherits": "inherits_from", "inherit": "inherits_from", "inherited": "inherits_from",
    "depends": "depends_on", "depend": "depends_on", "depended": "depends_on", "depending": "depends_on",
    "belongs": "belongs_to", "belong": "belongs_to", "belonged": "belongs_to",
    "produces": "produces", "produce": "produces", "produced": "produces", "producing": "produces",
    "converts": "converts_to", "convert": "converts_to", "converted": "converts_to", "converting": "converts_to",
    "stores": "stores", "store": "stores", "stored": "stores", "storing": "stores",
    "loads": "loads", "load": "loads", "loaded": "loads", "loading": "loads",
    "executes": "executes", "execute": "executes", "executed": "executes", "executing": "executes",
    "wraps": "wraps", "wrap": "wraps", "wrapped": "wraps", "wrapping": "wraps",
    "queries": "queries", "query": "queries", "queried": "queries", "querying": "queries",
    "validates": "validates", "validate": "validates", "validated": "validates",
    "transforms": "transforms", "transform": "transforms", "transformed": "transforms",
    "maps": "maps_to", "map": "maps_to", "mapped": "maps_to", "mapping": "maps_to",
    "handles": "handles", "handle": "handles", "handled": "handles", "handling": "handles",
    "serves": "serves", "serve": "serves", "served": "serves", "serving": "serves",
    "offers": "offers", "offer": "offers", "offered": "offers", "offering": "offers",
    "accepts": "accepts", "accept": "accepts", "accepted": "accepts", "accepting": "accepts",
    "rejects": "rejects", "reject": "rejects", "rejected": "rejects", "rejecting": "rejects",
    "speaks": "speaks", "speak": "speaks", "spoke": "speaks",
    "works": "works_with", "work": "works_with", "worked": "works_with", "working": "works_with",
    "requires": "requires", "require": "requires", "required": "requires", "requiring": "requires",
    "allows": "allows", "allow": "allows", "allowed": "allows", "allowing": "allows",
    "makes": "makes", "make": "makes", "made": "makes", "making": "makes",
    "lists": "lists", "list": "lists", "listed": "lists", "listing": "lists",
    "shows": "shows", "show": "shows", "showed": "shows", "shown": "shows", "showing": "shows",
    "renders": "renders", "render": "renders", "rendered": "renders", "rendering": "renders",
    "fetches": "fetches", "fetch": "fetches", "fetched": "fetches", "fetching": "fetches",
    "merges": "merges", "merge": "merges", "merged": "merges", "merging": "merges",
    "logs": "logs", "log": "logs", "logged": "logs", "logging": "logs",
    "prints": "prints", "print": "prints", "printed": "prints", "printing": "prints",
    "tests": "tests", "test": "tests", "tested": "tests", "testing": "tests",
    "compiles": "compiles", "compile": "compiles", "compiled": "compiles", "compiling": "compiles",
    "builds": "builds", "build": "builds", "building": "builds",
    "deploys": "deploys_to", "deploy": "deploys_to", "deployed": "deployed_on", "deploying": "deploys_to",
    "installs": "installs_in", "install": "installs_in", "installed": "installed_in", "installing": "installs_in",
    "configures": "configures", "configure": "configures", "configured": "configures", "configuring": "configures",
    "updates": "updates", "update": "updates", "updated": "updates", "updating": "updates",
    "routes": "routes_to", "route": "routes_to", "routed": "routes_to", "routing": "routes_to",
    "deploys": "deploys_to", "deploy": "deploys_to", "deployed": "deployed_on", "deploying": "deploys_to",
    "hosts": "hosts", "host": "hosts", "hosted": "hosted_on", "hosting": "hosts",
    "integrates": "integrates_with", "integrate": "integrates_with", "integrated": "integrated_with",
    "analyses": "analyses", "analyse": "analyses", "analyze": "analyses", "analyzed": "analyses",
    "extracts": "extracts_from", "extract": "extracts_from", "extracted": "extracted_from",
    "detects": "detects", "detect": "detects", "detected": "detects",
    "embeds": "embeds", "embed": "embeds", "embedded": "embedded_in",
    "trains": "trains_on", "train": "trains_on", "trained": "trained_on", "training": "trains_on",
    "learns": "learns_from", "learn": "learns_from", "learned": "learns_from", "learning": "learns_from",
    # Spanish
    "es": "is_a", "son": "is_a", "era": "is_a", "eran": "is_a",
    "tiene": "has_feature", "tienen": "has_feature", "tener": "has_feature",
    "usa": "uses", "usan": "uses", "usado": "uses", "utiliza": "uses", "utilizan": "uses",
    "proporciona": "provides", "proporcionan": "provides",
    "contiene": "contains", "contienen": "contains",
    "incluye": "includes", "incluyen": "includes",
    "construido": "is_built_with", "construye": "is_built_with", "construir": "is_built_with",
    "crea": "creates", "crean": "creates", "creado": "created_by", "crear": "creates",
    "ejecuta": "runs_on", "ejecutan": "runs_on", "ejecutar": "runs_on",
    "soporta": "supports", "soportan": "supports",
    "significa": "means", "significan": "means",
    "representa": "represents", "representan": "represents",
    "define": "defines", "definen": "defines",
    "implementa": "implements", "implementan": "implements",
    "depende": "depends_on", "dependen": "depends_on",
    "pertenece": "belongs_to", "pertenecen": "belongs_to",
    "produce": "produces", "producen": "produces",
    "convierte": "converts_to", "convierten": "converts_to",
    "almacena": "stores", "almacenan": "stores",
    "carga": "loads", "cargar": "loads", "cargado": "loads",
    "procesa": "processes", "procesan": "processes", "procesar": "processes",
    "genera": "generates", "generan": "generates", "generar": "generates",
    "gestiona": "manages", "gestionan": "manages",
    "analiza": "analyses", "analizan": "analyses",
    "extrae": "extracts_from", "extraen": "extracts_from",
    "detecta": "detects", "detectan": "detects",
    "entrena": "trains_on", "entrenan": "trains_on",
}


def normalize_verb(verb: str) -> str:
    """Map a verb to its canonical predicate form."""
    v = verb.lower().strip()
    if v in _VERB_MAP:
        return _VERB_MAP[v]
    # Try stripping trailing 's'
    if v.endswith("s") and v[:-1] in _VERB_MAP:
        return _VERB_MAP[v[:-1]]
    # Try stripping 'ed'
    if v.endswith("ed") and v[:-2] in _VERB_MAP:
        return _VERB_MAP[v[:-2]]
    # Try stripping 'ing'
    if v.endswith("ing") and v[:-3] in _VERB_MAP:
        return _VERB_MAP[v[:-3]]
    return v

def split_into_sentences(text: str) -> List[str]:
    sentences = re.split(r'[.!?\n]+', text)
    return [s.strip() for s in sentences if len(s.strip()) > 10]

def extract_entities_regex(text: str) -> List[Dict]:
    """Rule-based entity extraction — no external dependencies."""
    raw = []
    for m in _ENTITY_REGEX.finditer(text):
        entity_text = m.group(0).strip()
        if entity_text in _STOPWORDS or len(entity_text) < 3:
            continue
        raw.append({
            'text': entity_text,
            'start': m.start(),
            'end': m.end(),
            'score': 0.5,
        })
    # Remove entities fully contained within a larger overlapping entity
    raw.sort(key=lambda e: (e['start'], -e['end']))
    entities = []
    for e in raw:
        if any(e['start'] >= o['start'] and e['end'] <= o['end'] and o is not e for o in raw):
            continue
        if e['text'] in {x['text'] for x in entities}:
            continue
        entities.append(e)
    return entities

def extract_entities(text: str) -> List[Dict]:
    """Extract entities — GLiNER if available, regex fallback otherwise."""
    model = get_model()
    if model is None:
        return extract_entities_regex(text)
    try:
        entities = model.predict_entities(text, ENTITY_LABELS, threshold=0.65)
        return [e for e in entities if e.get('score', 0) > 0.65]
    except Exception as e:
        print(f"Entity extraction error: {e}")
        return extract_entities_regex(text)

def _tokenize(text: str) -> list:
    """Split text into (word, start_char, end_char) tokens."""
    tokens = []
    pos = 0
    for m in re.finditer(r'\S+', text):
        tokens.append((m.group(), m.start(), m.end()))
    return tokens


def _find_verbs(tokens: list) -> list:
    """Return list of (word, token_index, char_start, char_end) for known verbs."""
    verbs = []
    for idx, (word, start, end) in enumerate(tokens):
        clean = word.strip('.,;:!?()[]{}"\'¡¿')
        if clean.lower() in _KNOWN_VERBS:
            verbs.append((clean, idx, start, end))
    return verbs


def _entities_at_tokens(tokens: list, entities: List[Dict]) -> dict:
    """Map token indices to entity dicts (best match per token).

    For multi-token entities (e.g. 'Claude Desktop'), maps ALL
    contained tokens to the same entity. Overlapping entities resolved
    by largest span.
    """
    ent_at = {}
    # Build entity→span mapping
    for idx, (word, w_start, w_end) in enumerate(tokens):
        best = None
        best_overlap = 0
        for ent in entities:
            e_start = ent.get("start", 0)
            e_end = ent.get("end", 0)
            overlap = min(w_end, e_end) - max(w_start, e_start)
            if overlap > best_overlap:
                best_overlap = overlap
                best = ent
        if best_overlap > 0:
            ent_at[idx] = best
        # Also match by text for tokens within multi-word entities
        clean = word.strip('.,;:!?()[]{}"\'¡¿')
        if clean not in ent_at or not best_overlap:
            for ent in entities:
                if ent.get("text", "").lower() == clean.lower():
                    ent_at[idx] = ent
                    break
    # Fill gaps: if a non-matched token sits between two tokens of the same entity,
    # assign it to that entity as well (handles partial entity spans)
    for ent in entities:
        e_text = ent.get("text", "").strip()
        if " " not in e_text:
            continue
        parts = e_text.lower().split()
        # Find first and last token index for this entity
        matched = [idx for idx, e in ent_at.items() if e is ent]
        if not matched:
            continue
        min_idx, max_idx = min(matched), max(matched)
        # Fill all tokens within the entity span
        for idx in range(min_idx, max_idx + 1):
            if idx not in ent_at:
                ent_at[idx] = ent
    return ent_at


def _extract_noun_phrases(tokens: list, v_idx: int) -> list:
    """Extract noun phrases after a verb (for copula: 'is a X', 'are Y Z')."""
    phrases = []
    i = v_idx + 1
    # Skip articles/determiners
    determiners = frozenset({"a", "an", "the", "this", "that", "these", "those", "some", "any"})
    while i < len(tokens):
        word = tokens[i][0].strip('.,;:!?()[]{}"\'¡¿').lower()
        if i == v_idx + 1 and word in determiners:
            i += 1
            continue
        if word in _KNOWN_VERBS:
            break  # stop at next verb
        # Collect up to 5 words as a phrase
        phrase_end = min(i + 5, len(tokens))
        phrase_words = []
        for j in range(i, phrase_end):
            w = tokens[j][0].strip('.,;:!?()[]{}"\'¡¿')
            if w.lower() in _KNOWN_VERBS or w.lower() in determiners:
                break
            phrase_words.append(w)
        if phrase_words:
            phrases.append({
                "text": " ".join(phrase_words),
                "score": 0.6,
            })
        break
    return phrases


def extract_svo_triples(sentence: str, entities: List[Dict]) -> List[Dict]:
    """Extract SVO triples using verb lexicon + entity/noun-phrase positions."""
    triples = []
    tokens = _tokenize(sentence)
    verbs = _find_verbs(tokens)
    ent_at = _entities_at_tokens(tokens, entities)

    for verb_word, v_idx, v_start, v_end in verbs:
        # Subject: nearest entity, domain noun, or capitalized word before the verb
        subject = None
        for idx in range(v_idx - 1, -1, -1):
            if idx in ent_at:
                subject = ent_at[idx]
                break
            word = tokens[idx][0].strip('.,;:!?()[]{}"\'¡¿')
            w_lower = word.lower()
            # Accept domain nouns as implicit entities
            if w_lower in _DOMAIN_NOUNS and w_lower not in _KNOWN_VERBS:
                subject = {"text": word, "score": 0.5}
                ent_at[idx] = subject
                break
            # Accept capitalized words as implicit subjects
            if (word[0].isupper() and word not in _STOPWORDS and len(word) >= 3
                    and w_lower not in _KNOWN_VERBS):
                subject = {"text": word, "score": 0.5}
                ent_at[idx] = subject
                break
            # Skip function words and keep scanning
            if w_lower in frozenset({"a", "an", "the", "in", "of", "for", "by",
                    "with", "this", "that", "to", "from", "and", "or", "on", "at",
                    "as", "is", "are", "was", "were", "be", "been"}):
                continue
            # Skip common lowercase words — keep scanning
            if word[0].islower() and len(word) >= 2:
                continue
            break

        if subject is None:
            continue

        # Object: nearest entity, domain noun, or capitalized word after verb
        obj = None
        for idx in range(v_idx + 1, len(tokens)):
            if idx in ent_at:
                obj = ent_at[idx]
                break
            word = tokens[idx][0].strip('.,;:!?()[]{}"\'¡¿')
            w_lower = word.lower()
            # Accept domain nouns as implicit entities
            if w_lower in _DOMAIN_NOUNS and w_lower not in _KNOWN_VERBS:
                obj = {"text": word, "score": 0.5}
                break
            # Accept capitalized word as implicit entity
            if (word[0].isupper() and word not in _STOPWORDS and len(word) >= 2
                    and w_lower not in _KNOWN_VERBS):
                # Check for compound entity
                compound = word
                j = idx + 1
                while j < len(tokens):
                    w = tokens[j][0].strip('.,;:!?()[]{}"\'¡¿')
                    if w.lower() in frozenset({"is", "are", "was", "were", "in",
                            "of", "for", "by", "the", "a", "an", "and", "or",
                            "on", "at", "to", "from"}):
                        break
                    if w.lower() in _KNOWN_VERBS:
                        break
                    compound += " " + w
                    j += 1
                obj = {"text": compound, "score": 0.6}
                break
            # Accept domain noun phrases: detect multi-word objects
            if w_lower in _DOMAIN_NOUNS:
                # Scan forward to collect any following domain nouns
                compound = word
                j = idx + 1
                while j < len(tokens):
                    w = tokens[j][0].strip('.,;:!?()[]{}"\'¡¿')
                    if w.lower() not in _DOMAIN_NOUNS or w.lower() in _KNOWN_VERBS:
                        break
                    compound += " " + w
                    j += 1
                obj = {"text": compound, "score": 0.5}
                break
            # Skip function words and keep scanning
            if w_lower in frozenset({"a", "an", "the", "in", "of", "for", "by",
                    "with", "this", "that", "to", "from", "and", "or", "on", "at",
                    "as", "is", "are", "was", "were", "be", "been"}):
                continue
            # Skip lowercase words — keep scanning
            if word[0].islower() and len(word) >= 2:
                continue
            break
        
        # If no entity-based object, try noun phrase extraction
        if obj is None:
            nps = _extract_noun_phrases(tokens, v_idx)
            if nps:
                obj = nps[0]

        if obj is None:
            continue

        predicate = normalize_verb(verb_word)
        s_text = subject["text"].strip()
        o_text = obj["text"].strip()
        if len(s_text) < 2 or len(o_text) < 2:
            continue
        confidence = min(subject.get("score", 0.5), obj.get("score", 0.5), 0.85)

        # Deduplicate
        seen = False
        for t in triples:
            if t["subject"] == s_text and t["object"] == o_text and t["predicate"] == predicate:
                seen = True
                break
        if not seen:
            triples.append({
                "subject": s_text,
                "predicate": predicate,
                "object": o_text,
                "confidence": round(confidence, 2),
            })

    return triples


def extract_entity_pair_triples(sentence: str, entities: List[Dict], max_pairs: int = 3) -> List[Dict]:
    """Fallback: create triples from entity pairs when SVO fails."""
    triples = []
    tokens = _tokenize(sentence)
    words = [t[0] for t in tokens]

    # Sort entities by position
    sorted_ents = sorted(entities, key=lambda e: e.get("start", 0))
    pair_count = 0

    for i in range(len(sorted_ents)):
        for j in range(i + 1, len(sorted_ents)):
            ent_a = sorted_ents[i]
            ent_b = sorted_ents[j]
            a_text = ent_a.get("text", "").strip()
            b_text = ent_b.get("text", "").strip()
            if len(a_text) < 2 or len(b_text) < 2:
                continue

            # Determine predicate from word between them (on surviving surface)
            a_end = ent_a.get("end", 0)
            b_start = ent_b.get("start", 0)
            between_words = []
            for word in words:
                pass
            # Simple: check for 'of', 'in', 'by', 'for' between them
            gap = sentence[a_end:b_start].strip().lower()
            if " of " in gap:
                predicate = "member_of"
            elif " by " in gap:
                predicate = "created_by"
            elif " in " in gap:
                predicate = "located_in"
            elif " for " in gap:
                predicate = "used_for"
            else:
                predicate = "relates_to"

            triple = {
                "subject": a_text,
                "predicate": predicate,
                "object": b_text,
                "confidence": round(min(ent_a.get("score", 0.5), ent_b.get("score", 0.5)), 2),
            }

            # Deduplicate
            seen = False
            for t in triples:
                if (t["subject"] == triple["subject"] and t["object"] == triple["object"]
                        and t["predicate"] == triple["predicate"]):
                    seen = True
                    break
            if not seen:
                triples.append(triple)
                pair_count += 1
                if pair_count >= max_pairs:
                    return triples

    return triples


def extract_triples_from_text(text: str, max_triples: int = 5) -> List[Dict]:
    """Extract subject-predicate-object triples from text.

    Uses SVO (Subject-Verb-Object) extraction first, then falls back
    to entity-pair relations when no verb is found.
    """
    triples = []
    sentences = split_into_sentences(text)

    for sentence in sentences:
        if not sentence:
            continue

        entities = extract_entities(sentence)

        # Phase 1: SVO extraction (verb-driven)
        svo = extract_svo_triples(sentence, entities)
        for st in svo:
            seen = False
            for t in triples:
                if (t["subject"] == st["subject"] and t["object"] == st["object"]
                        and t["predicate"] == st["predicate"]):
                    seen = True
                    break
            if not seen:
                triples.append(st)
                if len(triples) >= max_triples:
                    return triples

        # Phase 2: entity-pair fallback (only if SVO found nothing)
        if not svo and len(entities) >= 2:
            pairs = extract_entity_pair_triples(sentence, entities)
            for pt in pairs:
                seen = False
                for t in triples:
                    if (t["subject"] == pt["subject"] and t["object"] == pt["object"]
                            and t["predicate"] == pt["predicate"]):
                        seen = True
                        break
                if not seen:
                    triples.append(pt)
                    if len(triples) >= max_triples:
                        return triples

    return triples
    
    return triples

@mcp.tool()
async def extract_triples(
    text: str,
    context: str = "",
    max_triples: int = 5,
) -> str:
    """Extract subject-predicate-object triples from text for knowledge graph.
    
    Use for: extract knowledge, build knowledge graph, extract relations,
    find entities, extract triples, analyze text for facts, knowledge extraction.
    
    Args:
        text: The text to extract triples from.
        context: Optional context or domain information.
        max_triples: Maximum number of triples to return (default 5).
    
    Returns:
        JSON string with extracted triples and confidence scores.
    """
    if not text:
        return json.dumps({"triples": [], "error": "No text provided"})

    full_text = (context + ". " + text) if context else text

    try:
        triples = extract_triples_from_text(full_text, max_triples)
        result = {
            "triples": triples,
            "count": len(triples),
            "model": "fastino/gliner2-base-v1" if GLINER_AVAILABLE else "regex-fallback",
            "threshold": 0.65 if GLINER_AVAILABLE else 0.5,
        }
        return json.dumps(result, ensure_ascii=False, indent=2)
    except Exception as e:
        return json.dumps({"triples": [], "error": f"Extraction failed: {str(e)}"})

from guilds.core import utils

if __name__ == "__main__":
    utils.safe_mcp_run(mcp)