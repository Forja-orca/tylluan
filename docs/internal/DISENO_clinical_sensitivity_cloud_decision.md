# PROPUESTA — Dónde comprobar `DataSensitivity::Clinical` antes de usar adaptadores cloud

Estado: propuesta escrita (T-DSH-120c), **sin código**. Base: T-DSH-119 aceptada.

## Problema

Los adaptadores OpenAI/Anthropic/Ollama son transporte puro y hoy no miran la sensibilidad. La regla de ADR-020 E-4 exige **HITL por petición** para `Clinical` hacia nube, y el wire de decisión no lleva hoy un campo de sensibilidad: si el punto de decisión es cloud, una petición clínica podría salir sin esa puerta.

## Dónde debe vivir la comprobación

**En la frontera del servicio de decisión, no en los adaptadores.** Concretamente en `router/decision/service.rs`, al construir el `ModelSlot`/al ejecutar `decide()`:

1. `resolve_model_slot` ya conoce la localidad efectiva del punto (`point.is_cloud()`), así que puede envolver el proveedor cloud en un **decorador** `ClinicalHtlGate<P: DecisionProvider>` (mismo trait, sin tocar los adaptadores).
2. El decorador, en `decide()`, **antes de cualquier HTTP**:
   - calcula la sensibilidad de la petición (ver abajo);
   - si es `Clinical` y el proveedor es cloud: busca una **aprobación HITL por petición** (dispatch approve) para ese `decision_row_hash`; sin aprobación ⇒ `DecisionError::Abstained("clinical_requires_hitl")` (nunca llama al proveedor); con aprobación ⇒ delega.
   - si no es `Clinical` (Public/Private) o el proveedor es local ⇒ delega.
3. El decorador es genérico y reutilizable: `ClinicalHtlGate::new(inner, provider_id)`; no cambia el contrato `DecisionProvider` ni los adaptadores.

## De dónde sale la sensibilidad

- **Corto plazo (sin cambiar el wire)**: derivarla del `state` de la petición (`metadata.space`, `metadata.sensitivity`, o herencia de recall como en ADR-020 §3.2.B). Si no se puede determinar y el proveedor es cloud ⇒ **fail-closed**: tratarla como `Clinical` y exigir HITL (hasta que exista canal explícito).
- **Medio plazo (recomendado)**: añadir al wire `DecideRequest` un campo opcional `sensitivity` (aditivo, compatible) que el llamante rellene; el servicio lo propaga al decorador. Esto evita inferencias frágiles desde `state`.

## Reutilización HITL

- Ya existe la cola de dispatches con `/api/v1/dispatches/{id}/approve|reject` y el patrón de aprobación hash-bound. El gate debe:
  - registrar la petición pendiente (hash `decision_row_hash` + proveedor + punto) y devolver abstención con la razón estable `clinical_requires_hitl`;
  - al aprobar, permitir **una** ejecución (no aprobaciones persistentes ni globales);
  - auditar el intento (quién, punto, proveedor, hash) sin registrar el payload clínico.
- Nunca debe existir un "modo permisivo" global: la excepción es por petición y por punto cloud.

## Tests previstos (para cuando se implemente)

1. Cloud + Clinical sin aprobación ⇒ `Abstained("clinical_requires_hitl")` y **0 peticiones** al servidor falso.
2. Cloud + Clinical con aprobación de esa petición ⇒ 1 petición.
3. Cloud + Public/Private con `cloud_enabled=true` ⇒ pasa (sin HITL).
4. Local (loopback) + Clinical ⇒ pasa sin HITL.
5. Sensibilidad desconocida + cloud ⇒ fail-closed (HITL).
6. La aprobación de una petición **no** autoriza otra (hash distinto).

## Nota

Mientras esto no esté implementado, la recomendación operativa es **no configurar puntos de decisión cloud** para espacios clínicos; el aviso debe quedar en la doc del registro de puntos. La implementación es pequeña (decorador + campo opcional de wire) y no requiere tocar los adaptadores.
