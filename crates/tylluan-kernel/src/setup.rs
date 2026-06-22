use std::path::PathBuf;

pub fn find_workspace_root() -> PathBuf {
    let current_dir = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
    let mut root = current_dir.clone();
    for _ in 0..5 {
        if root.join("tylluan.toml").exists() || root.join(".venv").exists() || root.join("guilds").exists() {
            return root;
        }
        if let Some(parent) = root.parent() {
            root = parent.to_path_buf();
        } else {
            break;
        }
    }
    current_dir
}

pub fn get_cli_arg(args: &[String], flag: &str) -> Option<String> {
    args.iter().position(|a| a == flag).and_then(|p| args.get(p + 1)).cloned()
}

#[cfg(target_os = "windows")]
pub fn setup_windows_job_object() {
    use std::ptr;
    use winapi::um::jobapi2::{CreateJobObjectW, AssignProcessToJobObject, SetInformationJobObject};
    use winapi::um::winnt::{
        JOBOBJECT_EXTENDED_LIMIT_INFORMATION, JobObjectExtendedLimitInformation,
        JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE,
    };
    use winapi::um::processthreadsapi::GetCurrentProcess;
    unsafe {
        let job = CreateJobObjectW(ptr::null_mut(), ptr::null());
        if job.is_null() {
            tracing::warn!("Could not create job object");
            return;
        }
        let mut info: JOBOBJECT_EXTENDED_LIMIT_INFORMATION = std::mem::zeroed();
        info.BasicLimitInformation.LimitFlags = JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE;
        SetInformationJobObject(job, JobObjectExtendedLimitInformation, &mut info as *mut _ as *mut winapi::ctypes::c_void, std::mem::size_of::<JOBOBJECT_EXTENDED_LIMIT_INFORMATION>() as u32);
        AssignProcessToJobObject(job, GetCurrentProcess());
        tracing::info!("Windows job object set: kill-on-job-close");
    }
}
