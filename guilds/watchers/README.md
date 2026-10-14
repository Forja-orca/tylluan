# Tylluan Autonomous Watchers

Background agents that listen to the coloquio SSE stream and respond to @mentions
when an autonomous session is active. Zero LLM cost when dormant.

## Architecture

```
[Session inactive]  watcher polls /api/v1/agents/session every 30s → sleeps → 0 tokens
[Session active]    watcher opens SSE /api/v1/events → filters coloquio:new_turn
                    → detects @agent_id → checks budget gate → calls LLM → posts response
```

## Quick start

```powershell
# 1. Start a session (activates the watcher)
curl -X POST http://127.0.0.1:3033/api/v1/agents/session/start `
  -H "Authorization: Bearer <token>" `
  -H "Content-Type: application/json" `
  -d '{"agent_id":"haiku","max_responses":10}'

# 2. Run the watcher (pick one)
cd E:\tylluan\guilds\watchers
python openai_compat_watcher.py haiku   # any OpenAI-compat backend

# 3. Stop the session (watcher goes back to sleep)
curl -X POST http://127.0.0.1:3033/api/v1/agents/session/stop `
  -H "Authorization: Bearer <token>" `
  -d '{"agent_id":"haiku"}'
```

## Watcher files

| File | Backend | API key needed |
|------|---------|----------------|
| `openai_compat_watcher.py` | Any `/v1/chat/completions` endpoint | Optional (local = no) |
| `lmstudio_watcher.py` | LM Studio localhost:1234 | No |
| `claude_watcher.py` | Anthropic API (Haiku) | Yes — `ANTHROPIC_API_KEY` |
| `antigravity_watcher.py` | Google Gemini API | Yes — `GEMINI_API_KEY` |

**Recommended**: use `openai_compat_watcher.py` — works with any provider.

## Provider configuration (openai_compat_watcher.py)

| Provider | WATCHER_LLM_URL | WATCHER_LLM_MODEL | WATCHER_LLM_KEY |
|----------|-----------------|-------------------|-----------------|
| LM Studio (default) | `http://127.0.0.1:1234` | any loaded model | `lm-studio` |
| Ollama | `http://127.0.0.1:11434` | `llama3.2` / `qwen2.5` | `ollama` |
| Hermes API server | `http://127.0.0.1:8642` | `hermes-3` | `API_SERVER_KEY` value |
| OpenAI | `https://api.openai.com` | `gpt-4o-mini` | `sk-...` |
| OpenRouter | `https://openrouter.ai/api` | `meta-llama/...` | `sk-or-...` |
| vLLM / llama.cpp | `http://127.0.0.1:8000` | model name | `none` |

```powershell
# Ollama example
$env:WATCHER_LLM_URL   = "http://127.0.0.1:11434"
$env:WATCHER_LLM_MODEL = "qwen2.5:7b"
$env:WATCHER_LLM_KEY   = "ollama"
python openai_compat_watcher.py haiku

# Hermes API server (requires API_SERVER_ENABLED=true when launching Hermes)
# Port 8642, Bearer token = whatever you set in API_SERVER_KEY
$env:WATCHER_LLM_URL   = "http://127.0.0.1:8642"
$env:WATCHER_LLM_MODEL = "hermes-3"
$env:WATCHER_LLM_KEY   = "<your API_SERVER_KEY>"
python openai_compat_watcher.py haiku
```

## Auth (FORJA_TOKEN)

> **Nota (2026-10-14).** Los watchers del repo no leen `FORJA_TOKEN` ni `.tylluan-token`: no hay descubrimiento automático de token en este código. Con `dev_mode = true` no hace falta token; con auth activa, hay que añadir la cabecera a mano.

`FORJA_URL` cambia la URL del kernel. Su valor por defecto en el código es `http://127.0.0.1:3033`, que es el puerto del host en Docker; para el kernel nativo (puerto `47004`) hay que fijar `FORJA_URL=http://127.0.0.1:47004`.

## Budget safety

Each watcher session has:
- **TTL**: 7200s (2h) — auto-expires
- **max_responses**: configurable per session (default: 50)
- **STOP**: `POST /api/v1/agents/session/stop` or dashboard toggle
- **Gate**: heartbeat endpoint blocks further responses when budget exhausted

Zero LLM calls when gate is closed.
