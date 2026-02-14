use anyhow::Result;
use clap::Parser;
use crossterm::{
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::{backend::CrosstermBackend, Terminal};
use std::io;

/// RAII guard that restores terminal state on drop (panic, early return, etc.)
struct TerminalGuard;

impl Drop for TerminalGuard {
    fn drop(&mut self) {
        let _ = disable_raw_mode();
        let _ = execute!(io::stdout(), LeaveAlternateScreen);
        let _ = execute!(io::stdout(), crossterm::cursor::Show);
    }
}

mod app;
mod event_loop;
mod input;
mod k8s;
pub mod models;
pub mod state;
mod ui;
pub mod utils;

#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
struct Args {
    /// Run as a one-off CLI command
    #[arg(short, long)]
    command: Option<String>,
}

fn init_tracing(to_file: bool) {
    let filter = tracing_subscriber::EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("kr=info,kube=warn,hyper=warn,tower=warn,h2=warn"));

    if to_file {
        // TUI mode: write logs to file to avoid corrupting the terminal
        let log_dir = dirs::config_dir()
            .unwrap_or_else(|| std::path::PathBuf::from("."))
            .join("kr");
        let _ = std::fs::create_dir_all(&log_dir);
        if let Ok(file) = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(log_dir.join("kr.log"))
        {
            tracing_subscriber::fmt()
                .with_env_filter(filter)
                .with_writer(std::sync::Mutex::new(file))
                .with_ansi(false)
                .init();
            return;
        }
    }

    // CLI mode (or file open failed): write to stderr
    tracing_subscriber::fmt().with_env_filter(filter).init();
}

#[tokio::main]
async fn main() -> Result<()> {
    let args = Args::parse();

    if let Some(cmd) = args.command {
        init_tracing(false);

        // CLI Mode — parse with shlex for proper quoting support
        let args_vec = match shlex::split(&cmd) {
            Some(args) => args,
            None => {
                eprintln!("Failed to parse command: unmatched quotes");
                std::process::exit(1);
            }
        };
        let status = std::process::Command::new("kubectl")
            .args(&args_vec)
            .status();

        match status {
            Ok(s) => {
                if !s.success() {
                    eprintln!("Command failed with status: {}", s);
                }
            }
            Err(e) => {
                eprintln!("Failed to execute kubectl: {}", e);
            }
        }
        return Ok(());
    }

    // TUI Mode — logs go to ~/.config/kr/kr.log
    init_tracing(true);

    // Create kube client BEFORE entering TUI so exec auth plugins
    // (e.g. Teleport tsh) can interact with the terminal for SSO/MFA.
    eprintln!("Connecting to cluster...");
    let client = k8s::client::default_client().await?;

    // Install panic hook to restore terminal on panic
    let original_hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |panic_info| {
        let _ = disable_raw_mode();
        let _ = execute!(io::stdout(), LeaveAlternateScreen);
        let _ = execute!(io::stdout(), crossterm::cursor::Show);
        original_hook(panic_info);
    }));

    enable_raw_mode()?;
    let _guard = TerminalGuard; // restores terminal on any exit path

    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    // Initialize App and Run Event Loop
    let (app, event_rx) = app::App::new(client).await?;
    event_loop::run(&mut terminal, app, event_rx).await?;

    Ok(())
}
