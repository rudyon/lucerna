use clap::Parser;
use lux8_asm::assemble;
use std::path::PathBuf;

/// An assembler for the lux8 processor
#[derive(Parser, Debug)]
#[command(author, version, about)]
struct Args {
    /// Assembly source file to read
    input: PathBuf,

    /// Binary output file to write
    #[arg(short, long, default_value = "a.rom")]
    output: PathBuf,
}

fn main() -> Result<(), String> {
    let args = Args::parse();

    let source = std::fs::read_to_string(&args.input)
        .map_err(|error| format!("Could not read {}: {error}", args.input.display()))?;

    println!("Read {} bytes from {}", source.len(), args.input.display());
    let image = assemble(&source)?;
    std::fs::write(&args.output, &image)
        .map_err(|error| format!("Could not write {}: {error}", args.output.display()))?;

    println!("Wrote {} bytes to {}", image.len(), args.output.display());
    Ok(())
}
