//! T-DSH-47 — las declaraciones `exec_tools` cubren toda la tabla de falsos
//! negativos de T-DSH-44/T-DSH-45 (sin implementarlas).
//!
//! El test fija un **diccionario Rust** con la declaración propuesta para cada
//! herramienta que ejecuta procesos y aún no declara `exec_tools` (todas las FN
//! de las tablas de T-DSH-44 en `guilds/core` + T-DSH-45 fuera de core, menos
//! browser/mcp_bridge que ya lo declaran desde T-DSH-46), y comprueba contra los
//! `.py` reales:
//!   1. cada herramienta nombrada EXISTE como `@mcp.tool` en su fichero;
//!   2. no queda ninguna FN de las tablas sin declarar (cobertura completa);
//!   3. `cron_schedule` es la única con `command_arg` (el comando lo elige el
//!      llamador y se ejecuta al disparar) y la única marcada para contención
//!      (`strict = true`); el resto son uso normal del producto (git, docker,
//!      ffmpeg, comfy, llama, clipboard, screenshot, sandbox, search, biome) y
//!      NO deberían declararse con `process_execution=false` (romperían Strict).
//!
//! No modifica ningún `.py`: solo lee y afirma. La propuesta completa con bloques
//! listos para pegar está en `IMPL_T-DSH-47_declaraciones_exec_tools_2026-10-23.md`.
//!
//! Run: rustup run 1.99.1 cargo test -p tylluan-kernel --test exec_tools_declarations_cover_fn_table -- --nocapture

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

struct Decl {
    /// Ruta relativa a la raíz del repo.
    file: &'static str,
    tool: &'static str,
    /// Argumento del que el llamador aporta el comando ejecutable, si existe.
    command_arg: Option<&'static str>,
    /// ¿Debería bloquearse bajo Strict/`process_execution=false`?
    strict: bool,
    why: &'static str,
}

/// Propuesta de declaración para las FN que aún no tienen `exec_tools`.
/// - `strict = true`: contiene un comando elegido por el llamador → contención.
/// - `strict = false`: ejecuta como parte del uso normal del producto → no debe
///   declararse con `process_execution=false` (Strict rompería la funcionalidad).
const DECLARATIONS: [Decl; 33] = [
    // guilds/core (T-DSH-44 FN menos browser/mcp_bridge, ya declaradas en T-DSH-46)
    Decl { file: "guilds/core/comfy_ui.py", tool: "generate_short", command_arg: None, strict: false, why: "montaje de video con ffmpeg (uso normal)" },
    Decl { file: "guilds/core/comfy_ui.py", tool: "generate_documentary_video", command_arg: None, strict: false, why: "montaje de video con ffmpeg (uso normal)" },
    Decl { file: "guilds/core/llama_backend.py", tool: "query_model", command_arg: None, strict: false, why: "auto-arranca llama-server (inferencia, uso normal)" },
    // builders/plugins
    Decl { file: "guilds/builders/plugins/git.py", tool: "git_add", command_arg: None, strict: false, why: "subprocess git (uso normal)" },
    Decl { file: "guilds/builders/plugins/git.py", tool: "git_branch", command_arg: None, strict: false, why: "subprocess git (uso normal)" },
    Decl { file: "guilds/builders/plugins/git.py", tool: "git_commit", command_arg: None, strict: false, why: "subprocess git (uso normal)" },
    Decl { file: "guilds/builders/plugins/git.py", tool: "git_diff", command_arg: None, strict: false, why: "subprocess git (uso normal)" },
    Decl { file: "guilds/builders/plugins/git.py", tool: "git_log", command_arg: None, strict: false, why: "subprocess git (uso normal)" },
    Decl { file: "guilds/builders/plugins/git.py", tool: "git_status", command_arg: None, strict: false, why: "subprocess git (uso normal)" },
    Decl { file: "guilds/builders/plugins/git.py", tool: "git_status_quick", command_arg: None, strict: false, why: "subprocess git (uso normal)" },
    Decl { file: "guilds/builders/plugins/docker.py", tool: "docker_images", command_arg: None, strict: false, why: "subprocess docker (uso normal)" },
    Decl { file: "guilds/builders/plugins/docker.py", tool: "docker_logs", command_arg: None, strict: false, why: "subprocess docker (uso normal)" },
    Decl { file: "guilds/builders/plugins/docker.py", tool: "docker_ps", command_arg: None, strict: false, why: "subprocess docker (uso normal)" },
    Decl { file: "guilds/builders/plugins/docker.py", tool: "docker_status", command_arg: None, strict: false, why: "subprocess docker (uso normal)" },
    Decl { file: "guilds/builders/plugins/docker.py", tool: "docker_stop", command_arg: None, strict: false, why: "subprocess docker (uso normal)" },
    Decl { file: "guilds/builders/plugins/ffmpeg_tools.py", tool: "audio_extract", command_arg: None, strict: false, why: "ffmpeg (uso normal)" },
    Decl { file: "guilds/builders/plugins/ffmpeg_tools.py", tool: "media_probe", command_arg: None, strict: false, why: "ffprobe/ffmpeg (uso normal)" },
    Decl { file: "guilds/builders/plugins/ffmpeg_tools.py", tool: "video_add_text", command_arg: None, strict: false, why: "ffmpeg (uso normal)" },
    Decl { file: "guilds/builders/plugins/ffmpeg_tools.py", tool: "video_concat", command_arg: None, strict: false, why: "ffmpeg (uso normal)" },
    Decl { file: "guilds/builders/plugins/ffmpeg_tools.py", tool: "video_resize", command_arg: None, strict: false, why: "ffmpeg (uso normal)" },
    Decl { file: "guilds/builders/plugins/ffmpeg_tools.py", tool: "video_trim", command_arg: None, strict: false, why: "ffmpeg (uso normal)" },
    Decl { file: "guilds/builders/plugins/clipboard_tools.py", tool: "clipboard_read", command_arg: None, strict: false, why: "powershell/pbpaste/xclip (uso normal)" },
    Decl { file: "guilds/builders/plugins/clipboard_tools.py", tool: "clipboard_write", command_arg: None, strict: false, why: "powershell/pbcopy/xclip (uso normal)" },
    Decl { file: "guilds/builders/plugins/screenshot_tools.py", tool: "screenshot_capture", command_arg: None, strict: false, why: "powershell/screencapture/import (uso normal)" },
    Decl { file: "guilds/builders/plugins/sandbox.py", tool: "sandbox_ingest", command_arg: None, strict: false, why: "docker run del sandbox (el sandbox ES la contención)" },
    Decl { file: "guilds/builders/plugins/sandbox.py", tool: "sandbox_python", command_arg: None, strict: false, why: "docker run del sandbox (el sandbox ES la contención)" },
    Decl { file: "guilds/builders/plugins/sandbox.py", tool: "sandbox_status", command_arg: None, strict: false, why: "docker info/kill del sandbox (uso normal)" },
    // scholars / wardens / watchers
    Decl { file: "guilds/scholars/plugins/search.py", tool: "search_code_structure", command_arg: None, strict: false, why: "ripgrep vía utils.run_command (uso normal)" },
    Decl { file: "guilds/scholars/plugins/search.py", tool: "search_content", command_arg: None, strict: false, why: "ripgrep vía utils.run_command (uso normal)" },
    Decl { file: "guilds/wardens/plugins/biome_warden.py", tool: "biome_format_file", command_arg: None, strict: false, why: "binario biome (uso normal)" },
    Decl { file: "guilds/wardens/plugins/biome_warden.py", tool: "biome_lint_file", command_arg: None, strict: false, why: "binario biome (uso normal)" },
    Decl { file: "guilds/watchers/plugins/cron_scheduler.py", tool: "cron_schedule", command_arg: Some("command"), strict: true, why: "el comando lo aporta el llamador y se ejecuta en cada disparo (cron_scheduler.py:52)" },
    Decl { file: "guilds/watchers/plugins/cron_scheduler.py", tool: "cron_cancel", command_arg: None, strict: false, why: "ejecución indirecta al cargar timers (_init); cancelar es uso normal" },
];

/// FN de las tablas de T-DSH-44 (core) y T-DSH-45 (no-core) que NO tienen aún
/// `exec_tools` (se excluyen browser/mcp_bridge, declaradas en T-DSH-46).
const FN_TABLE: [(&str, &str); 33] = [
    ("guilds/core/comfy_ui.py", "generate_short"),
    ("guilds/core/comfy_ui.py", "generate_documentary_video"),
    ("guilds/core/llama_backend.py", "query_model"),
    ("guilds/builders/plugins/git.py", "git_add"),
    ("guilds/builders/plugins/git.py", "git_branch"),
    ("guilds/builders/plugins/git.py", "git_commit"),
    ("guilds/builders/plugins/git.py", "git_diff"),
    ("guilds/builders/plugins/git.py", "git_log"),
    ("guilds/builders/plugins/git.py", "git_status"),
    ("guilds/builders/plugins/git.py", "git_status_quick"),
    ("guilds/builders/plugins/docker.py", "docker_images"),
    ("guilds/builders/plugins/docker.py", "docker_logs"),
    ("guilds/builders/plugins/docker.py", "docker_ps"),
    ("guilds/builders/plugins/docker.py", "docker_status"),
    ("guilds/builders/plugins/docker.py", "docker_stop"),
    ("guilds/builders/plugins/ffmpeg_tools.py", "audio_extract"),
    ("guilds/builders/plugins/ffmpeg_tools.py", "media_probe"),
    ("guilds/builders/plugins/ffmpeg_tools.py", "video_add_text"),
    ("guilds/builders/plugins/ffmpeg_tools.py", "video_concat"),
    ("guilds/builders/plugins/ffmpeg_tools.py", "video_resize"),
    ("guilds/builders/plugins/ffmpeg_tools.py", "video_trim"),
    ("guilds/builders/plugins/clipboard_tools.py", "clipboard_read"),
    ("guilds/builders/plugins/clipboard_tools.py", "clipboard_write"),
    ("guilds/builders/plugins/screenshot_tools.py", "screenshot_capture"),
    ("guilds/builders/plugins/sandbox.py", "sandbox_ingest"),
    ("guilds/builders/plugins/sandbox.py", "sandbox_python"),
    ("guilds/builders/plugins/sandbox.py", "sandbox_status"),
    ("guilds/scholars/plugins/search.py", "search_code_structure"),
    ("guilds/scholars/plugins/search.py", "search_content"),
    ("guilds/wardens/plugins/biome_warden.py", "biome_format_file"),
    ("guilds/wardens/plugins/biome_warden.py", "biome_lint_file"),
    ("guilds/watchers/plugins/cron_scheduler.py", "cron_schedule"),
    ("guilds/watchers/plugins/cron_scheduler.py", "cron_cancel"),
];

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

/// ¿Existe `@mcp.tool` + `def <tool>(` en el fichero?
fn tool_exists(lines: &[String], tool: &str) -> bool {
    let mut found = false;
    for (i, line) in lines.iter().enumerate() {
        let t = line.trim_start();
        if !(t.starts_with("def ") || t.starts_with("async def ")) {
            continue;
        }
        let after = t
            .trim_start_matches("async ")
            .trim_start_matches("def ")
            .trim_start();
        if !after.starts_with(tool) {
            continue;
        }
        let rest = &after[tool.len()..];
        if !rest.trim_start().starts_with('(') {
            continue;
        }
        // Decorador contiguo por encima.
        let mut k = i;
        while k > 0 {
            k -= 1;
            let dec = lines[k].trim_start();
            if dec.starts_with('@') {
                if dec.contains("mcp.tool") || dec.contains("app.tool") || dec.starts_with("@tool") {
                    found = true;
                }
                continue;
            }
            break;
        }
    }
    found
}

#[test]
fn exec_tools_declarations_cover_the_fn_table() {
    let root = repo_root();
    let mut declared: BTreeSet<(&str, &str)> = BTreeSet::new();
    let mut strict_tools: Vec<(&str, &str)> = Vec::new();

    println!(
        "{:<44} {:<30} {:<14} {:<6} motivo",
        "fichero", "herramienta", "command_arg", "strict"
    );
    for d in DECLARATIONS {
        let path = root.join(d.file);
        let content = std::fs::read_to_string(&path)
            .unwrap_or_else(|e| panic!("no se pudo leer {}: {e}", path.display()));
        let lines: Vec<String> = content.lines().map(str::to_string).collect();
        assert!(
            tool_exists(&lines, d.tool),
            "la herramienta '{}' no existe como @mcp.tool en {}",
            d.tool,
            d.file
        );
        assert!(
            declared.insert((d.file, d.tool)),
            "declaración duplicada para {}.{}",
            d.file,
            d.tool
        );
        if d.strict {
            strict_tools.push((d.file, d.tool));
        }
        println!(
            "{:<44} {:<30} {:<14} {:<6} {}",
            d.file,
            d.tool,
            d.command_arg.unwrap_or("-"),
            d.strict,
            d.why
        );
    }

    // (2) Cobertura: ninguna FN de las tablas queda sin declarar.
    let fn_table: BTreeSet<(&str, &str)> = FN_TABLE.iter().copied().collect();
    let missing: Vec<&(&str, &str)> = fn_table.difference(&declared).collect();
    let extra: Vec<&(&str, &str)> = declared.difference(&fn_table).collect();
    println!(
        "\ndeclaradas={} FN_tabla={} sin_declarar={:?} declaradas_que_no_son_FN={:?}",
        declared.len(),
        fn_table.len(),
        missing,
        extra
    );
    assert!(
        missing.is_empty(),
        "hay FN de las tablas sin declarar en la propuesta: {missing:?}"
    );
    assert!(
        extra.is_empty(),
        "la propuesta declara herramientas que no están en las tablas FN: {extra:?}"
    );

    // (3) Solo cron_schedule tiene comando del llamador y es la única para
    //     contención estricta; cron_schedule declara command_arg = "command".
    assert_eq!(
        strict_tools,
        vec![("guilds/watchers/plugins/cron_scheduler.py", "cron_schedule")],
        "solo el comando de cron_schedule justifica contención estricta"
    );
    let with_command_arg: Vec<&str> = DECLARATIONS
        .iter()
        .filter(|d| d.command_arg.is_some())
        .map(|d| d.tool)
        .collect();
    assert_eq!(with_command_arg, vec!["cron_schedule"]);
    assert_eq!(
        DECLARATIONS
            .iter()
            .find(|d| d.tool == "cron_schedule")
            .and_then(|d| d.command_arg),
        Some("command")
    );

    // La firma real de cron_schedule recibe el comando.
    let cron = std::fs::read_to_string(root.join("guilds/watchers/plugins/cron_scheduler.py"))
        .expect("cron_scheduler.py");
    assert!(
        cron.contains("async def cron_schedule(name: str, command: str, interval_minutes: int"),
        "cron_schedule debe recibir 'command: str' del llamador"
    );
    println!(
        "\nOK: {} declaraciones cubren la tabla FN; única estricta: cron_schedule (command_arg=command)",
        declared.len()
    );
}
