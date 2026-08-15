"""Echo A2A agent served with the OFFICIAL a2a-sdk (v1.1.2).

Used to verify Tylluan's outbound A2A client (crates/tylluan-kernel
src/transport/http/a2a_client.rs) against a reference implementation of the
A2A spec v0.3.0, not against our own code.

Run (from the repo root):
    pip install "a2a-sdk[fastapi]" uvicorn
    python tools/a2a_echo_agent.py

Then probe it with the kernel's client. The agent echoes the last text part
of the incoming message with an "echo:" prefix.

Note: the v1.1.2 SDK's `AgentCard` proto no longer carries a `url` field
(card url is injected by the route layer via card_modifier). Tylluan's client
falls back to the configured base URL when the card has no url.
"""

import asyncio
import sys

from a2a.server.agent_execution.agent_executor import AgentExecutor, EventQueue
from a2a.server.agent_execution.context import RequestContext
from a2a.server.request_handlers.default_request_handler_v2 import DefaultRequestHandlerV2
from a2a.server.routes.agent_card_routes import create_agent_card_routes
from a2a.server.routes.fastapi_routes import add_a2a_routes_to_fastapi
from a2a.server.routes.jsonrpc_routes import create_jsonrpc_routes
from a2a.server.tasks.inmemory_task_store import InMemoryTaskStore
from a2a.types.a2a_pb2 import AgentCard, Message, Part, Task, TaskState, TaskStatus

HOST = "127.0.0.1"
PORT = 8901
ROLE_USER = 1
ROLE_AGENT = 2


class EchoExecutor(AgentExecutor):
    """Task executor that immediately completes with an echo message."""

    async def execute(self, context: RequestContext, event_queue: EventQueue) -> None:
        text = ""
        for part in context.message.parts:
            if part.WhichOneof("content") == "text":
                text = part.text
                break
        reply = Message(role=ROLE_AGENT, parts=[Part(text=f"echo: {text}")])
        await event_queue.enqueue_event(Task(
            id=context.task_id,
            context_id=context.context_id,
            status=TaskStatus(state=TaskState.TASK_STATE_COMPLETED, message=reply),
        ))

    async def cancel(self, context: RequestContext, event_queue: EventQueue) -> None:
        await event_queue.enqueue_event(Task(
            id=context.task_id,
            context_id=context.context_id,
            status=TaskStatus(state=TaskState.TASK_STATE_CANCELED),
        ))


def build_card() -> AgentCard:
    card = AgentCard(
        name="sdk-echo",
        description="Official a2a-sdk echo agent used to verify Tylluan's outbound client",
        version="1.1.2",
    )
    skill = card.skills.add()
    skill.id = "echo.text"
    skill.name = "echo.text"
    skill.description = "Echoes the incoming text message"
    card.default_input_modes.append("text")
    card.default_output_modes.append("text")
    card.capabilities.streaming = True
    card.capabilities.push_notifications = False
    card.capabilities.extended_agent_card = False
    return card


async def main() -> None:
    from fastapi import FastAPI

    task_store = InMemoryTaskStore()
    handler = DefaultRequestHandlerV2(
        agent_executor=EchoExecutor(),
        task_store=task_store,
        agent_card=build_card(),
    )

    async def card_modifier(card: AgentCard) -> AgentCard:
        card.description = (
            "Official a2a-sdk echo agent used to verify Tylluan's outbound client"
        )
        return card

    app = FastAPI(title="sdk-echo (official a2a-sdk)")
    add_a2a_routes_to_fastapi(
        app,
        agent_card_routes=create_agent_card_routes(build_card(), card_modifier=card_modifier),
        jsonrpc_routes=create_jsonrpc_routes(
            handler,
            rpc_url="/a2a",
            # Tylluan's client speaks the A2A v0.3.0 wire format
            # (message/send, tasks/get, tasks/cancel). The SDK's v0.3 compat
            # adapter maps those onto the v1.x handler methods.
            enable_v0_3_compat=True,
        ),
    )

    import uvicorn

    config = uvicorn.Config(app, host=HOST, port=PORT, log_level="warning")
    server = uvicorn.Server(config)
    await server.serve()


if __name__ == "__main__":
    try:
        asyncio.run(main())
    except KeyboardInterrupt:
        sys.exit(0)