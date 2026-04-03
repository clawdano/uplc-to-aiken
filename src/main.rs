use clap::{Parser, Subcommand};
use std::path::PathBuf;

mod blockfrost;
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

    /// Fetch and decompile scripts from Blockfrost
    Fetch {
        /// Blockfrost API key (or set BLOCKFROST_API_KEY env var)
        #[arg(long, env = "BLOCKFROST_API_KEY")]
        api_key: String,

        /// Script hash to fetch and decompile
        #[arg(long, group = "fetch_mode")]
        script_hash: Option<String>,

        /// Fetch and decompile N recent Plutus V2 scripts
        #[arg(long, group = "fetch_mode")]
        recent_v2: Option<usize>,

        /// Network (mainnet, preprod, preview)
        #[arg(long, default_value = "mainnet")]
        network: String,

        /// Output directory for decompiled scripts
        #[arg(short, long)]
        output_dir: Option<PathBuf>,
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

            let ir_program = ir::lower(&uplc_program);

            if show_ir {
                println!("{:#?}", ir_program);
                return Ok(());
            }

            let optimized = decompiler::decompile(ir_program);
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

        Commands::Fetch {
            api_key,
            script_hash,
            recent_v2,
            network,
            output_dir,
        } => {
            let client = blockfrost::BlockfrostClient::new(&api_key, &network);

            if let Some(hash) = script_hash {
                // Fetch a single script by hash
                eprintln!("Fetching script {}...", hash);
                let info = client
                    .get_script_info(&hash)
                    .map_err(|e| miette::miette!("Failed to fetch script info: {}", e))?;
                eprintln!("  Type: {}", info.script_type);

                let cbor = client
                    .get_script_cbor(&hash)
                    .map_err(|e| miette::miette!("Failed to fetch script CBOR: {}", e))?;

                match decompile_cbor_hex(&cbor) {
                    Ok(source) => {
                        if let Some(ref dir) = output_dir {
                            std::fs::create_dir_all(dir)
                                .map_err(|e| miette::miette!("Failed to create dir: {}", e))?;
                            let path = dir.join(format!("{}.ak", &hash[..12]));
                            std::fs::write(&path, &source)
                                .map_err(|e| miette::miette!("Failed to write: {}", e))?;
                            eprintln!("  Wrote {}", path.display());
                        } else {
                            println!("// Script: {}", hash);
                            println!("// Type: {}", info.script_type);
                            println!();
                            println!("{}", source);
                        }
                    }
                    Err(e) => {
                        eprintln!("  Failed to decompile: {}", e);
                    }
                }
            } else if let Some(count) = recent_v2 {
                // Fetch recent Plutus V2 scripts
                eprintln!(
                    "Fetching up to {} recent Plutus V2 scripts from {}...",
                    count, network
                );
                let scripts = client
                    .fetch_plutus_v2_scripts(count)
                    .map_err(|e| miette::miette!("Failed to fetch scripts: {}", e))?;

                eprintln!("Found {} Plutus V2 scripts", scripts.len());

                for (hash, cbor) in &scripts {
                    eprint!("  {} ... ", &hash[..12]);
                    match decompile_cbor_hex(cbor) {
                        Ok(source) => {
                            if let Some(ref dir) = output_dir {
                                std::fs::create_dir_all(dir)
                                    .map_err(|e| miette::miette!("Failed to create dir: {}", e))?;
                                let path = dir.join(format!("{}.ak", &hash[..12]));
                                std::fs::write(&path, &source)
                                    .map_err(|e| miette::miette!("Failed to write: {}", e))?;
                                eprintln!("OK ({} lines)", source.lines().count());
                            } else {
                                println!("// Script: {}", hash);
                                println!("{}", source);
                                println!();
                                eprintln!("OK");
                            }
                        }
                        Err(e) => {
                            eprintln!("FAILED: {}", e);
                        }
                    }
                }
            } else {
                eprintln!("Error: provide either --script-hash or --recent-v2");
                std::process::exit(1);
            }

            Ok(())
        }
    }
}

fn decompile_cbor_hex(hex_str: &str) -> Result<String, String> {
    let program = parser::parse_from_cbor_hex(hex_str).map_err(|e| format!("{}", e))?;
    let ir = ir::lower(&program);
    let optimized = decompiler::decompile(ir);
    Ok(codegen::emit(&optimized))
}
