use tracing::info;

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

    let mut sys = System::new();
    sys.refresh_processes(ProcessesToUpdate::All, true);
    
    let self_pid = std::process::id();
    let guild_pattern = guild_name.replace('-', "_");
    
    let mut target_pid = None;

    for (pid, process) in sys.processes() {
        if let Some(parent_pid) = process.parent()
            && parent_pid.as_u32() == self_pid {
                let cmd = process.cmd().iter()
                    .map(|s| s.to_string_lossy().to_string())
                    .collect::<Vec<_>>()
                    .join(" ");
                let cmd_lower = cmd.to_lowercase();
                let name_lower = guild_name.to_lowercase();
                let pattern_lower = guild_pattern.to_lowercase();
                
                let is_guild = cmd.contains("guilds.") || cmd.contains("guilds/") || cmd.contains("guilds\\");
                let matches_name = cmd_lower.contains(&name_lower) 
                    || cmd_lower.contains(&pattern_lower)
                    || (guild_name == "scrapling" && cmd_lower.contains("scrapling_web"));
                    
                if is_guild && matches_name {
                    target_pid = Some(pid.as_u32());
                    break;
                }
            }
    }

    let pid = match target_pid {
        Some(p) => p,
        None => return Err(anyhow::anyhow!("Could not find child process for guild '{}'", guild_name)),
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
            return Err(anyhow::anyhow!("OpenProcess failed for PID {}", pid));
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
pub fn isolate_guild_process(guild_name: &str, cpu_limit_pct: u32) -> anyhow::Result<()> {
    warn!("🛡️ Process isolation not supported on this OS (Guild '{}')", guild_name);
    Ok(())
}
