#!/usr/bin/env bash
# H-47C gate: los 7 sitios medidos (H-47A) no pueden volver a loguear texto
# crudo de intent/query/payload. El diseño está en
# docs/internal/IMPL_H-47A_hermes_2026-10.md (§5 B1-B4) y la implementación
# en crates/tylluan-kernel/src/logging.rs (redacted/safe/mcp_payload/scrub).
# Si alguien reintroduce una interpolación directa, este gate falla.
set -u
cd "$(dirname "$0")/.."

fail=0
check_absent() {
    local pat="$1" why="$2" hits
    hits=$(grep -rnF --include='*.rs' -- "$pat" crates/tylluan-kernel/src || true)
    if [ -n "$hits" ]; then
        echo "DRIFT: '$pat' ha vuelto (${why}):"
        echo "$hits"
        fail=1
    fi
}

# Literales exactos que H-47A encontró en prod; ahora sirven de sello en negativo.
check_absent 'Received payload: {}'        'mcp.rs debe usar logging::mcp_payload'
check_absent 'intent = %intent,'           'handler_do/mod.rs debe usar logging::redacted'
check_absent 'intent='"'"'{}'"'"''         'prepare.rs debe usar logging::redacted'
check_absent '🎯 Routing: '"'"'{}'"'"''     'routing.rs debe usar logging::redacted'
check_absent 'query = %query,'             'handler_recall.rs debe usar logging::redacted'
check_absent 'hybrid scoring '"'"'{}'"'"'' 'matcher.rs debe usar logging::redacted'
check_absent 'intent = %obs.intent,'       'observe.rs debe usar logging::redacted'

if [ "$fail" -eq 0 ]; then
    echo "OK: redaccion de logs en vigor (7 sellos H-47A, helper logging::*)."
fi
exit $fail
