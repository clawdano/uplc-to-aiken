use clap::{Parser, Subcommand};
use std::path::PathBuf;

mod codegen;
mod decompiler;
mod ir;
mod parser;

#[derive(Parser)]
#[command(
    name = "uplc-to-aiken",
    about = "UPLC bytecode decompiler targeting Aiken",
    long_about = "Reverse-engineer Cardano smart contracts from UPLC bytecode into readable Aiken code.\n\n\
                  Supports both CBOR hex (on-chain format) and text-format UPLC as input.\n\
                  Currently targets Plutus V2 scripts with output in latest Aiken syntax.",
    version
)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Decompile a UPLC script to Aiken source code
    Decompile {
        /// Path to a UPLC file (text format or CBOR hex)
        #[arg(short, long, group = "input_source")]
        input: Option<PathBuf>,

        /// CBOR hex string to decompile
        #[arg(long, group = "input_source")]
        hex: Option<String>,

        /// Output file path (defaults to stdout)
        #[arg(short, long)]
        output: Option<PathBuf>,

        /// Plutus version of the input script
        #[arg(long, default_value = "v2")]
        plutus_version: PlutusVersion,

        /// Show the intermediate representation instead of Aiken
        #[arg(long)]
        show_ir: bool,

        /// Show the raw UPLC AST
        #[arg(long)]
        show_ast: bool,
    },
}

#[derive(Clone, Debug, clap::ValueEnum)]
enum PlutusVersion {
    V1,
    V2,
    V3,
}

fn main() -> miette::Result<()> {
    let cli = Cli::parse();

    match cli.command {
        Commands::Decompile {
            input,
            hex,
            output,
            plutus_version: _plutus_version,
            show_ir,
            show_ast,
        } => {
            let uplc_program = match (&input, &hex) {
                (Some(path), None) => parser::parse_from_file(path)?,
                (None, Some(hex_str)) => parser::parse_from_cbor_hex(hex_str)?,
                (None, None) => {
                    eprintln!("Error: provide either --input <file> or --hex <cbor_hex>");
                    std::process::exit(1);
                }
                _ => unreachable!("clap group prevents both"),
            };

            if show_ast {
                println!("{:#?}", uplc_program);
                return Ok(());
            }

            // Lower UPLC to our IR
            let ir_program = ir::lower(&uplc_program);

            if show_ir {
                println!("{:#?}", ir_program);
                return Ok(());
            }

            // Run decompilation passes (pattern recognition)
            let optimized = decompiler::decompile(ir_program);

            // Generate Aiken source code
            let aiken_source = codegen::emit(&optimized);

            match output {
                Some(path) => {
                    std::fs::write(&path, &aiken_source)
                        .map_err(|e| miette::miette!("Failed to write output: {}", e))?;
                    eprintln!("Wrote decompiled Aiken to {}", path.display());
                }
                None => {
                    print!("{}", aiken_source);
                }
            }

            Ok(())
        }
    }
}
