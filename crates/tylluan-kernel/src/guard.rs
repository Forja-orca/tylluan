//! # GuardedTask Subsystem
//! 
//! Provides a resilient wrapper for asynchronous tasks, designed for hardware 
//! with limited resources. Prevents kernel locks by monitoring task execution 
//! and extending timeouts if the task is still alive but slow.

use std::time::Duration;
use tokio::time::timeout;
use tracing::{warn, error};
use std::future::Future;
use anyhow::{Result, anyhow};

pub struct GuardedTask {
    pub name: String,
    pub initial_timeout: Duration,
    pub max_extensions: u32,
}

impl GuardedTask {
    pub fn new(name: &str, initial_timeout: Duration) -> Self {
        Self {
            name: name.to_string(),
            initial_timeout,
            max_extensions: 3, // Default tolerance for slow hardware
        }
    }

    /// Execute a task with an adaptive timeout guard and a tokio::spawn handle.
    /// Requires 'static lifetime for background execution.
    pub async fn run<F, T>(&self, task_future: F) -> Result<T> 
    where 
        F: Future<Output = Result<T>> + Send + 'static,
        T: Send + 'static
    {
        let mut handle = tokio::spawn(task_future);
        let mut extensions_used = 0;
        let mut current_timeout = self.initial_timeout;

        loop {
            match timeout(current_timeout, &mut handle).await {
                Ok(result) => {
                    // Task finished within timeout
                    let join_res: Result<T> = result.map_err(|e| anyhow!("Task panic or cancel: {}", e))?;
                    return join_res;
                }
                Err(_) => {
                    // Timeout expired, but is it dead or just slow?
                    if extensions_used < self.max_extensions {
                        extensions_used += 1;
                        warn!(
                            "⏳ Task '{}' (background) taking longer than expected ({:?}). Extending guard ({} of {})...", 
                            self.name, current_timeout, extensions_used, self.max_extensions
                        );
                        // Backoff: Give it slightly more time in each extension
                        current_timeout = current_timeout.mul_f32(1.5);
                    } else {
                        error!(
                            "🛑 Task '{}' (background) exceeded total guard time. Forcing abandonment to protect Kernel integrity.", 
                            self.name
                        );
                        // We abort the handle to prevent resource leaks
                        handle.abort();
                        return Err(anyhow!("TIMEOUT: Task '{}' (background) exceeded resilience guard ({:?} + {} extensions).", self.name, self.initial_timeout, extensions_used));
                    }
                }
            }
        }
    }

    /// Execute a task with an adaptive timeout guard, without spawning (supports local borrows).
    pub async fn run_local<F, T>(&self, mut task_future: F) -> Result<T> 
    where 
        F: Future<Output = Result<T>> + std::marker::Unpin,
    {
        let mut extensions_used = 0;
        let mut current_timeout = self.initial_timeout;

        loop {
            match timeout(current_timeout, &mut task_future).await {
                Ok(result) => {
                    return result;
                }
                Err(_) => {
                    if extensions_used < self.max_extensions {
                        extensions_used += 1;
                        warn!(
                            "⏳ Task '{}' (local) taking longer than expected ({:?}). Extending guard ({} of {})...", 
                            self.name, current_timeout, extensions_used, self.max_extensions
                        );
                        current_timeout = current_timeout.mul_f32(1.5);
                    } else {
                        error!(
                            "🛑 Task '{}' (local) exceeded total guard time. Dropping future.", 
                            self.name
                        );
                        return Err(anyhow!("TIMEOUT: Task '{}' (local) exceeded resilience guard ({:?} + {} extensions).", self.name, self.initial_timeout, extensions_used));
                    }
                }
            }
        }
    }
}
