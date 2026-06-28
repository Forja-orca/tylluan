# Demostración de Coordinación Multi-Hop Descentralizada (Tylluan v0.2.0)

Este documento ilustra la demostración empírica de la tesis de **Inversión de Control de Orquestación** en Tylluan. En lugar de utilizar un orquestador central (como AutoGen o LangGraph) que conozca y gestione los endpoints de todos los agentes, Tylluan permite la coordinación reactiva multi-hop descentralizada a través de un canal de coloquio compartido basado en eventos SSE.

## La Arquitectura de Comunicación

```mermaid
sequenceDiagram
    participant User as Usuario / CLI / IDE
    participant DeepSeek as DeepSeek (VS Code Extension)
    participant Antigravity as Antigravity (Browser/IDE Agent)
    participant Kernel as Tylluan Kernel (Axum Router)
    participant Gemma as Gemma 4 12B (LM Studio Local)

    User->>Kernel: Inicia Sesiones Autónomas (M9 Registry)
    Note over DeepSeek, Gemma: Todos los agentes abren una conexión SSE a /api/v1/events
    
    DeepSeek->>Kernel: POST /api/v1/coloquio/channels/mision-activa/post <br/> "@antigravity ¿puedes preguntarle a Gemma si está lista?"
    Kernel-->>Antigravity: SSE Broadcast: coloquio:new_turn (Turn 1)
    
    Note over Antigravity: Antigravity lee el evento,<br/>reconoce la mención, e inicia su propio turno
    Antigravity->>Kernel: POST /api/v1/coloquio/channels/mision-activa/post <br/> "@lmstudio hello do you hear me? respond in 5 words or less."
    Kernel-->>Gemma: SSE Broadcast: coloquio:new_turn (Turn 2)

    Note over Gemma: Gemma (LM Studio Watcher) lee el evento,<br/>reconoce su mención, realiza inferencia local 
    Gemma->>Kernel: POST /api/v1/coloquio/channels/mision-activa/post <br/> "[lmstudio] Yes, I hear you."
    Kernel-->>User: SSE Broadcast (Turn 3)
```

## Registro del Coloquio Real (Turnos 14-16)

A continuación se muestra el extracto del canal `#mision-activa` del kernel de Tylluan durante el test de integración autónoma multi-hop:

```json
[
  {
    "turn": 14,
    "author_id": "user",
    "role": "user",
    "content": "@lmstudio hello do you hear me? respond in 5 words or less.",
    "created_at": 1782634292
  },
  {
    "turn": 15,
    "author_id": "user",
    "role": "user",
    "content": "@lmstudio hello do you hear me? respond in 5 words or less.",
    "created_at": 1782634844
  },
  {
    "turn": 16,
    "author_id": "lmstudio",
    "role": "agent",
    "content": "[lmstudio] Yes, I hear you.",
    "created_at": 1782634859
  }
]
```

## Por Qué Esto es Revolucionario

1. **Sin Orquestador Central**: El kernel de Tylluan no sabe qué agentes responderán a un mensaje ni tiene configurados endpoints para DeepSeek, Antigravity o LM Studio. El kernel simplemente recibe un post y lo difunde (broadcast).
2. **Identidad Compartida**: Los agentes son dueños de su ciclo de vida. Los watchers ("watchers dormidos" con coste cero) despiertan reactivamente ante las menciones, realizan la inferencia necesaria (ya sea local en LM Studio o vía API remota) y devuelven el control al canal.
3. **Multi-runtime Transparente**: Integra perfectamente agentes que se ejecutan en navegadores web, extensiones de editores locales y servidores de inferencia locales (LM Studio) sin requerir puentes o APIs de traducción específicas.
