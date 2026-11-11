use clap::{Parser, Subcommand};
use daytile::{build_calendar, read_events, Aggregation, SvgOptions, WeekStart};
use std::fs::{self, OpenOptions};
use std::io::{BufReader, Write};
use std::path::{Path, PathBuf};

#[derive(Debug, Parser)]
#[command(version, about)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Debug, Subcommand)]
enum Command {
    /// Render a standalone SVG calendar heatmap.
    Render {
        input: PathBuf,
        #[arg(short, long)]
        output: PathBuf,
        #[arg(long)]
        year: i32,
        #[arg(long, default_value = "sum")]
        aggregate: Aggregation,
        #[arg(long, default_value = "monday")]
        week_start: WeekStart,
        #[arg(long)]
        title: Option<String>,
        /// Five comma-separated #RRGGBB colors, low to high.
        #[arg(long, value_parser = parse_palette)]
        palette: Option<[String; 5]>,
        /// Replace an existing output file.
        #[arg(long)]
        force: bool,
    },
    /// Print an aggregated JSON summary.
    Summary {
        input: PathBuf,
        #[arg(long)]
        year: i32,
        #[arg(long, default_value = "sum")]
        aggregate: Aggregation,
    },
}

fn main() {
    if let Err(error) = run() {
        eprintln!("daytile: {error}");
        std::process::exit(1);
    }
}

fn run() -> Result<(), Box<dyn std::error::Error>> {
    match Cli::parse().command {
        Command::Render {
            input,
            output,
            year,
            aggregate,
            week_start,
            title,
            palette,
            force,
        } => {
            let calendar = load(&input, year, aggregate)?;
            let mut options = SvgOptions {
                title: title.unwrap_or_else(|| format!("{} calendar heatmap", year)),
                week_start,
                ..SvgOptions::default()
            };
            if let Some(palette) = palette {
                options.palette = palette;
            }
            let svg = calendar.render_svg(&options)?;
            write_output(&output, svg.as_bytes(), force)?;
            eprintln!(
                "rendered {} populated days to {}",
                calendar.days.len(),
                output.display()
            );
        }
        Command::Summary {
            input,
            year,
            aggregate,
        } => {
            let calendar = load(&input, year, aggregate)?;
            println!("{}", serde_json::to_string_pretty(&calendar.summary())?);
        }
    }
    Ok(())
}

fn load(
    path: &Path,
    year: i32,
    aggregation: Aggregation,
) -> Result<daytile::Calendar, Box<dyn std::error::Error>> {
    let file = fs::File::open(path)?;
    let events = read_events(BufReader::new(file))?;
    Ok(build_calendar(&events, year, aggregation))
}

fn write_output(path: &Path, contents: &[u8], force: bool) -> Result<(), std::io::Error> {
    let mut options = OpenOptions::new();
    options.write(true);
    if force {
        options.create(true).truncate(true);
    } else {
        options.create_new(true);
    }
    let mut file = options.open(path)?;
    file.write_all(contents)
}

fn parse_palette(value: &str) -> Result<[String; 5], String> {
    let colors: Vec<String> = value.split(',').map(str::to_owned).collect();
    colors
        .try_into()
        .map_err(|_| "palette requires exactly five comma-separated colors".to_owned())
}
