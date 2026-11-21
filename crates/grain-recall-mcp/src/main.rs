//! `grain-recall-mcp --data-dir <Grain app data folder>`
//!
//! Started by an AI app over stdio. stdout carries the MCP protocol only;
//! diagnostics go to stderr. Exits when the app closes the pipe.

use std::path::PathBuf;
use std::process::ExitCode;

use grain_recall_mcp::server::RecallServer;
use grain_recall_mcp::Recall;
use rmcp::transport::io::stdio;
use rmcp::ServiceExt;

const USAGE: &str = "Usage: grain-recall-mcp --data-dir <Grain app data folder>\n\
Grain's Recall settings write this for you when you connect an AI app.";

fn data_dir() -> Result<PathBuf, String> {
    let mut args = std::env::args_os().skip(1);
    let mut dir = None;
    while let Some(arg) = args.next() {
        match arg.to_str() {
            Some("--data-dir") => dir = args.next().map(PathBuf::from),
            Some("--version") => {
                println!("grain-recall-mcp {}", env!("CARGO_PKG_VERSION"));
                std::process::exit(0);
            }
            Some("--help" | "-h") => {
                println!("{USAGE}");
                std::process::exit(0);
            }
            _ => return Err(format!("unknown argument {arg:?}\n{USAGE}")),
        }
    }
    dir.filter(|d| !d.as_os_str().is_empty())
        .ok_or_else(|| USAGE.to_string())
}

fn main() -> ExitCode {
    let dir = match data_dir() {
        Ok(dir) => dir,
        Err(message) => {
            eprintln!("{message}");
            return ExitCode::from(2);
        }
    };
    // Tokio's stdin/stdout run on its blocking pool, so no I/O driver is
    // needed; rmcp's request handling uses timers.
    let runtime = match tokio::runtime::Builder::new_current_thread()
        .enable_time()
        .build()
    {
        Ok(runtime) => runtime,
        Err(error) => {
            eprintln!("grain-recall-mcp: could not start: {error}");
            return ExitCode::FAILURE;
        }
    };
    let served = runtime.block_on(async {
        let service = RecallServer::new(Recall::new(&dir))
            .serve(stdio())
            .await
            .map_err(|e| e.to_string())?;
        service.waiting().await.map_err(|e| e.to_string())?;
        Ok::<(), String>(())
    });
    match served {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("grain-recall-mcp: {error}");
            ExitCode::FAILURE
        }
    }
}
