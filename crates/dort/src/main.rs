mod cli;
mod environment;
mod services;
mod tui;

fn main() -> std::process::ExitCode {
    cli::run()
}
