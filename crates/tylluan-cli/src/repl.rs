use rustyline::error::ReadlineError;
use rustyline::DefaultEditor;
use colored::*;
use serde_json::json;
use reqwest::Client;
use anyhow::Result;

pub async fn run_repl() -> Result<()> {
    let mut rl = DefaultEditor::new()?;
    #[cfg(feature = "with-file-history")]
    if rl.load_history("history.txt").is_err() {
        // println!("No previous history.");
    }

    println!("{}", "🦉 TylluanNexus Sovereign Terminal".bold().blue());
    println!("{}", "Type your intent in natural language. Type 'exit' or 'quit' to leave.".dimmed());
    println!();

    let client = Client::new();
    let url = "http://127.0.0.1:3030/api/v1/do";
    
    // Attempt to read token
    let token = std::env::var("TYLLUAN_TOKEN")
        .ok()
        .or_else(|| std::fs::read_to_string(".tylluan-token").ok().map(|s| s.trim().to_string()))
        .unwrap_or_default();

    loop {
        let readline = rl.readline("tylluan> ");
        match readline {
            Ok(line) => {
                let line = line.trim();
                if line.is_empty() {
                    continue;
                }
                if line == "exit" || line == "quit" {
                    break;
                }
                rl.add_history_entry(line)?;

                println!("{} Thinking...", "🔍".blue());

                let res = client.post(url)
                    .header("Authorization", format!("Bearer {}", token))
                    .json(&json!({
                        "intent": line,
                        "agent_id": "tylluan-cli-repl"
                    }))
                    .send()
                    .await;

                match res {
                    Ok(resp) => {
                        if resp.status().is_success() {
                            let json_res: serde_json::Value = resp.json().await?;
                            let response = json_res["response"].as_str().unwrap_or("No response");
                            let is_error = json_res["is_error"].as_bool().unwrap_or(false);

                            if is_error {
                                println!("{} {}", "❌ Error:".red().bold(), response.red());
                            } else {
                                println!("\n{}\n", response.green());
                            }
                        } else {
                            println!("{} Hub returned error: {}", "❌".red(), resp.status());
                        }
                    }
                    Err(e) => {
                        println!("{} Could not connect to Hub: {}", "❌".red(), e);
                    }
                }
            }
            Err(ReadlineError::Interrupted) => {
                println!("CTRL-C");
                break;
            }
            Err(ReadlineError::Eof) => {
                println!("CTRL-D");
                break;
            }
            Err(err) => {
                println!("Error: {:?}", err);
                break;
            }
        }
    }
    
    #[cfg(feature = "with-file-history")]
    rl.save_history("history.txt")?;
    
    Ok(())
}
