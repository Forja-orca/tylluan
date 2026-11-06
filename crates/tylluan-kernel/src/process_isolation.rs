/// D-118: resumen de lo que vio el buscador de procesos cuando no encontró al
/// guild. Existe porque el fallo es **total**: en el log de producción
/// `E:\tylluan\logs\kernel.log` hay **0** líneas "Process isolation applied" frente a
/// **32** "Could not apply CPU isolation", para **19 guilds distintos**. O sea que
/// el tope de CPU del 20% no se ha aplicado nunca a nadie.
///
/// Con el mensaje solo ("Could not find child process") no se puede distinguir
/// entre las dos causas plausibles: (a) la regla de hijo directo, o (b) que
/// `sysinfo` no pueda leer la línea de comandos de esos procesos. Esto añade los
/// contadores al error para que la próxima vez el log lo diga solo, sin tener que
/// instrumentar a mano.
pub(crate) fn resumen_sonda(
    procesos: usize,
    cmd_vacias: usize,
    cmd_coincide: usize,
    hijos_directos: usize,
) -> String {
    format!(
        "procesos={procesos} cmd_vacias={cmd_vacias} cmd_coincide={cmd_coincide} \
         hijos_directos={hijos_directos} (cmd_coincide incluye sin exigir parentesco)"
    )
}

#[cfg(target_os = "windows")]
pub fn isolate_guild_process(guild_name: &str, cpu_limit_pct: u32) -> anyhow::Result<()> {
    use winapi::um::jobapi2::{CreateJobObjectW, SetInformationJobObject, AssignProcessToJobObject};
    use winapi::um::winnt::{
        JOBOBJECT_CPU_RATE_CONTROL_INFORMATION,
        JOB_OBJECT_CPU_RATE_CONTROL_ENABLE, JOB_OBJECT_CPU_RATE_CONTROL_HARD_CAP,
        JobObjectCpuRateControlInformation, PROCESS_SET_QUOTA, PROCESS_TERMINATE
    };
    use winapi::um::processthreadsapi::OpenProcess;
    use winapi::um::handleapi::CloseHandle;
    use std::ptr::null_mut;
    use sysinfo::{System, ProcessesToUpdate};
    use tracing::info;

    let mut sys = System::new();
    // D-119: `refresh_processes(All, true)` **no carga la línea de comandos** en
    // sysinfo 0.32: `cmd()` vuelve vacía para TODOS los procesos (medido: 543 de
    // 543). Sin la cmd, `is_guild` nunca es cierto y esta función fallaba el 100%
    // de las veces — en el log de producción, 0 "Process isolation applied" contra
    // 32 "Could not apply CPU isolation". Hay que pedirla explícitamente.
    sys.refresh_processes_specifics(
        ProcessesToUpdate::All,
        true,
        sysinfo::ProcessRefreshKind::everything(),
    );
    
    let self_pid = std::process::id();
    
    let mut target_pid = None;
    // D-118: contadores de la sonda (ver `resumen_sonda`).
    let mut sonda_procesos = 0usize;
    let mut sonda_cmd_vacias = 0usize;
    let mut sonda_cmd_coincide = 0usize;
    let mut sonda_hijos_directos = 0usize;

    for (pid, process) in sys.processes() {
        sonda_procesos += 1;
        let cmd = process.cmd().iter()
            .map(|s| s.to_string_lossy().to_string())
            .collect::<Vec<_>>()
            .join(" ");
        if cmd.trim().is_empty() {
            sonda_cmd_vacias += 1;
            continue;
        }
        // TL (revisión clase A, T2922): el MISMO bug que en `is_guild_orphan` estaba
        // aquí. Con `contains(nombre)` sobre toda la línea, pedir el aislamiento de
        // `code` podía capar el proceso de `code_graph`/`code_analysis`, y
        // `search` el de `websearch`/`deep_web_research`. Ahora se usa el matcher
        // exacto compartido (hoja del módulo), el mismo que el anti-huérfano.
        if !crate::registry::proxy::cmd_matches_guild_exact(&cmd, guild_name) {
            continue;
        }
        sonda_cmd_coincide += 1;

        if let Some(parent_pid) = process.parent()
            && parent_pid.as_u32() == self_pid {
            sonda_hijos_directos += 1;
            target_pid = Some(pid.as_u32());
            break;
        }
    }

    let pid = match target_pid {
        Some(p) => p,
        None => {
            return Err(anyhow::anyhow!(
                "Could not find child process for guild '{guild_name}' [{}]",
                resumen_sonda(
                    sonda_procesos,
                    sonda_cmd_vacias,
                    sonda_cmd_coincide,
                    sonda_hijos_directos
                )
            ))
        }
    };

    unsafe {
        let job = CreateJobObjectW(null_mut(), null_mut());
        if job.is_null() {
            return Err(anyhow::anyhow!("CreateJobObjectW failed"));
        }

        let mut info: JOBOBJECT_CPU_RATE_CONTROL_INFORMATION = std::mem::zeroed();
        info.ControlFlags = JOB_OBJECT_CPU_RATE_CONTROL_ENABLE | JOB_OBJECT_CPU_RATE_CONTROL_HARD_CAP;
        *info.u.CpuRate_mut() = cpu_limit_pct * 100;

        let res = SetInformationJobObject(
            job,
            JobObjectCpuRateControlInformation,
            &mut info as *mut _ as *mut winapi::ctypes::c_void,
            std::mem::size_of::<JOBOBJECT_CPU_RATE_CONTROL_INFORMATION>() as u32,
        );
        if res == 0 {
            CloseHandle(job);
            return Err(anyhow::anyhow!("SetInformationJobObject failed"));
        }

        let process_handle = OpenProcess(PROCESS_SET_QUOTA | PROCESS_TERMINATE, 0, pid);
        if process_handle.is_null() {
            CloseHandle(job);
            return Err(anyhow::anyhow!("OpenProcess failed for PID {pid}"));
        }

        let assign_res = AssignProcessToJobObject(job, process_handle);
        CloseHandle(process_handle);
        
        if assign_res == 0 {
            CloseHandle(job);
            return Err(anyhow::anyhow!("AssignProcessToJobObject failed"));
        }

        CloseHandle(job);
    }
    
    info!("🛡️ Process isolation applied to guild '{}' (PID {}): CPU limited to {}%", guild_name, pid, cpu_limit_pct);
    Ok(())
}

#[cfg(not(target_os = "windows"))]
pub fn isolate_guild_process(guild_name: &str, _cpu_limit_pct: u32) -> anyhow::Result<()> {
    use tracing::warn;
    warn!("🛡️ Process isolation not supported on this OS (Guild '{}')", guild_name);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Diagnóstico manual (no corre en CI): `cargo test -p tylluan-kernel --lib
    /// visor_sysinfo -- --ignored --nocapture`.
    ///
    /// D-118 dejó abiertas dos causas para el fallo 32/32 del aislamiento de CPU:
    /// (a) la regla de hijo directo, o (b) que `sysinfo` no lea la línea de comandos
    /// de esos procesos. Esto imprime lo que `sysinfo` ve de verdad sobre la tabla
    /// de procesos viva, para separarlas sin tocar producción.
    #[test]
    #[ignore = "diagnostico manual: lee la tabla de procesos viva"]
    fn visor_sysinfo() {
        use sysinfo::{ProcessesToUpdate, System};
        let mut sys = System::new();
        sys.refresh_processes(ProcessesToUpdate::All, true);

        let total = sys.processes().len();
        // Perspectiva REAL del kernel: su pid, sacado de la tabla viva.
        let kernel_pid = sys
            .processes()
            .values()
            .find(|p| {
                p.name()
                    .to_string_lossy()
                    .to_lowercase()
                    .contains("tylluan-nexus")
            })
            .map(|p| p.pid().as_u32())
            .unwrap_or(0);
        let mut vacias = 0usize;
        let mut de_guild = 0usize;
        let mut por_guild: std::collections::BTreeMap<String, Vec<(u32, u32, bool)>> =
            std::collections::BTreeMap::new();

        for (pid, process) in sys.processes() {
            let cmd = process
                .cmd()
                .iter()
                .map(|s| s.to_string_lossy().to_string())
                .collect::<Vec<_>>()
                .join(" ");
            if cmd.trim().is_empty() {
                vacias += 1;
                continue;
            }
            if !cmd.contains("guilds.") {
                continue;
            }
            de_guild += 1;
            let modulo = cmd
                .split("-m ")
                .nth(1)
                .map(|s| s.split_whitespace().next().unwrap_or("?").to_string())
                .unwrap_or_else(|| "?".to_string());
            let guild = modulo.rsplit('.').next().unwrap_or("?").to_string();
            let padre = process.parent().map(|p| p.as_u32()).unwrap_or(0);
            let nombre_casa = cmd.to_lowercase().contains(&guild.to_lowercase());
            por_guild
                .entry(guild)
                .or_default()
                .push((pid.as_u32(), padre, nombre_casa));
        }

        println!("[visor] procesos vistos por sysinfo: {total}");
        println!("[visor] con cmd() VACIA (refresh normal): {vacias}");
        println!("[visor] con 'guilds.' en la cmd   : {de_guild}");

        // D-123: cuanto CUESTA el refresh que arreglo D-119. Se llama dentro del
        // lock de escritura del registro (via `ensure_guild_running` -> proxy ->
        // `isolate_guild_process`), asi que su coste lo paga todo el que espera.
        let mut t_normal = Vec::new();
        for _ in 0..3 {
            let mut s = System::new();
            let t0 = std::time::Instant::now();
            s.refresh_processes(ProcessesToUpdate::All, true);
            t_normal.push(t0.elapsed().as_millis());
        }
        let mut t_completo = Vec::new();
        for _ in 0..3 {
            let mut s = System::new();
            let t0 = std::time::Instant::now();
            s.refresh_processes_specifics(
                ProcessesToUpdate::All,
                true,
                sysinfo::ProcessRefreshKind::everything(),
            );
            t_completo.push(t0.elapsed().as_millis());
        }
        println!("[coste] refresh normal    (3 pasadas, ms): {t_normal:?}");
        println!("[coste] refresh everything(3 pasadas, ms): {t_completo:?}");

        // Segunda pasada pidiendo EXPLICITAMENTE la linea de comandos.
        let mut sys2 = System::new();
        sys2.refresh_processes_specifics(
            ProcessesToUpdate::All,
            true,
            sysinfo::ProcessRefreshKind::everything(),
        );
        let total2 = sys2.processes().len();
        let vacias2 = sys2
            .processes()
            .values()
            .filter(|p| {
                p.cmd()
                    .iter()
                    .map(|s| s.to_string_lossy().to_string())
                    .collect::<Vec<_>>()
                    .join(" ")
                    .trim()
                    .is_empty()
            })
            .count();
        let de_guild2 = sys2
            .processes()
            .values()
            .filter(|p| {
                p.cmd()
                    .iter()
                    .map(|s| s.to_string_lossy().to_string())
                    .collect::<Vec<_>>()
                    .join(" ")
                    .contains("guilds.")
            })
            .count();
        println!("[visor] CON refresh_processes_specifics(everything):");
        println!("[visor]   procesos={total2} cmd_vacias={vacias2} con_'guilds.'={de_guild2}");

        // D-126: lo que de verdad importa antes de landear D-119 — a cuantos guilds
        // se les aplicara el tope de CPU del 20 % en el proximo arranque, usando el
        // refresh ARREGLADO y la regla REAL de `isolate_guild_process`.
        let mut cubiertos = 0usize;
        let mut sin_proceso: Vec<String> = Vec::new();
        let mut por_nombre: std::collections::BTreeSet<String> = std::collections::BTreeSet::new();
        for process in sys2.processes().values() {
            let cmd = process
                .cmd()
                .iter()
                .map(|s| s.to_string_lossy().to_string())
                .collect::<Vec<_>>()
                .join(" ");
            let Some(modulo) = cmd.split("-m ").nth(1) else {
                continue;
            };
            let modulo = modulo.split_whitespace().next().unwrap_or("").to_string();
            if !modulo.starts_with("guilds.") {
                continue;
            }
            let guild = modulo.rsplit('.').next().unwrap_or("").to_string();
            por_nombre.insert(guild);
        }
        for guild in &por_nombre {
            // La regla real: cmd_es_del_guild + hijo directo del kernel.
            let mut hallado = false;
            for process in sys2.processes().values() {
                let cmd = process
                    .cmd()
                    .iter()
                    .map(|s| s.to_string_lossy().to_string())
                    .collect::<Vec<_>>()
                    .join(" ");
                if !cmd.contains("guilds.") {
                    continue;
                }
                let casa = cmd.to_lowercase().contains(&guild.to_lowercase());
                let directo = process
                    .parent()
                    .map(|p| p.as_u32() == kernel_pid)
                    .unwrap_or(false);
                if casa && directo {
                    hallado = true;
                    break;
                }
            }
            if hallado {
                cubiertos += 1;
            } else {
                sin_proceso.push(guild.clone());
            }
        }
        println!(
            "[tope] con D-119, el tope del 20% se aplicaria a {cubiertos} de {} guilds",
            por_nombre.len()
        );
        if !sin_proceso.is_empty() {
            println!("[tope] sin proceso localizable: {sin_proceso:?}");
        }
        println!("[visor] este proceso (test) es pid : {}", std::process::id());
        println!("[visor] pid del KERNEL en ejecucion  : {kernel_pid}");
        for (guild, cands) in &por_guild {
            for (pid, padre, casa) in cands {
                let directo = *padre == kernel_pid;
                println!(
                    "[visor]   {guild:<18} pid={pid:<7} ppid={padre:<7} casa={casa} hijo_directo_del_kernel={directo}"
                );
            }
        }
        // Veredicto: ¿cuántos guilds hallaría el matcher tal como está escrito?
        let mut hallados = 0usize;
        for cands in por_guild.values() {
            if cands.iter().any(|(_, padre, casa)| *casa && *padre == kernel_pid) {
                hallados += 1;
            }
        }
        println!(
            "[visor] VEREDICTO: el matcher encontraria {hallados} de {} guilds",
            por_guild.len()
        );
    }


    /// D-118: el resumen tiene que llevar los cuatro números, para que el log de
    /// producción permita distinguir "el proceso no está" de "su cmd no se puede
    /// leer" sin volver a instrumentar nada.
    #[test]
    fn d118_el_resumen_lleva_los_cuatro_contadores() {
        let s = resumen_sonda(412, 7, 2, 0);
        for trozo in ["procesos=412", "cmd_vacias=7", "cmd_coincide=2", "hijos_directos=0"] {
            assert!(s.contains(trozo), "falta {trozo} en {s}");
        }
    }
}

#[cfg(test)]
mod tests_cableado_matcher {
    /// **T2922.** El aislamiento de CPU tenía su **propio** matcher por subcadena,
    /// copia del de `is_guild_orphan`: pedir el aislamiento de `code` podía capar el
    /// proceso de `code_graph`, y `search` el de `websearch`. Ahora comparte
    /// `cmd_matches_guild_exact`.
    ///
    /// Este test es **estructural** a propósito: la lógica ya la prueban los tests
    /// del matcher compartido, pero un test de comportamiento aquí exigiría arrancar
    /// procesos (aplica un Job Object de Windows). Lo que sí se puede fijar —y es lo
    /// que se rompió— es que este fichero **use** el matcher compartido y no vuelva
    /// a llevar el suyo.
    #[test]
    fn t2922_el_aislamiento_usa_el_matcher_exacto_compartido() {
        let src = include_str!("process_isolation.rs");
        let produccion = match src.find("\n#[cfg(test)]") {
            Some(i) => &src[..i],
            None => src,
        };
        assert!(
            produccion.contains("cmd_matches_guild_exact"),
            "el aislamiento tiene que usar el matcher exacto compartido"
        );
        assert!(
            !produccion.contains("matches_name"),
            "no puede volver el matcher por subcadena (el bug de T2922)"
        );
    }
}

#[cfg(test)]
mod tests_refresh {

    /// D-119: el refresh que usa este módulo tiene que dejar `cmd()` con contenido.
    /// Con `refresh_processes(All, true)` este test se pone **rojo**: sysinfo 0.32
    /// no carga la línea de comandos en el refresh normal.
    #[test]
    fn d119_el_refresh_carga_la_linea_de_comandos() {
        let mut sys = sysinfo::System::new();
        sys.refresh_processes_specifics(
            sysinfo::ProcessesToUpdate::All,
            true,
            sysinfo::ProcessRefreshKind::everything(),
        );
        let propio = sys
            .process(sysinfo::Pid::from_u32(std::process::id()))
            .expect("el test tiene que verse a si mismo");
        let cmd = propio
            .cmd()
            .iter()
            .map(|s| s.to_string_lossy().to_string())
            .collect::<Vec<_>>()
            .join(" ");
        assert!(
            !cmd.trim().is_empty(),
            "cmd() vacia: el refresh no esta pidiendo la linea de comandos (D-119)"
        );
    }
}
