mod app;
mod chunks;

use anyhow::{Context, Result};
use clap::Parser;
use std::path::{Path, PathBuf};

/// Rewrite a markdown file paragraph by paragraph, in your own words.
#[derive(Parser)]
#[command(version, about)]
struct Cli {
    /// Input markdown file
    input: PathBuf,

    /// Output file (default: <input stem>.rewritten.md next to input)
    #[arg(short, long)]
    output: Option<PathBuf>,
}

fn main() -> Result<()> {
    let cli = Cli::parse();
    let text = std::fs::read_to_string(&cli.input)
        .with_context(|| format!("cannot read {}", cli.input.display()))?;

    let chunks = chunks::split_chunks(&text);
    if chunks.is_empty() {
        anyhow::bail!("{} has no content", cli.input.display());
    }

    let output = cli
        .output
        .unwrap_or_else(|| default_output(&cli.input));

    let mut app = app::App::new(&cli.input, output, chunks);
    let mut terminal = ratatui::init();
    let result = app.run(&mut terminal);
    ratatui::restore();

    match result? {
        Some(path) => println!("Saved: {}", path.display()),
        None => println!("Aborted, nothing saved."),
    }
    Ok(())
}

fn default_output(input: &Path) -> PathBuf {
    let stem = input
        .file_stem()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_else(|| "output".into());
    input.with_file_name(format!("{stem}.rewritten.md"))
}
