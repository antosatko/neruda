use clap::{Parser, ValueEnum};
use parser::{
    grammar::gen_parser,
    lowering::{self, ModuleOk},
};
use std::{
    fs::File,
    io::{Read, Write},
    path::PathBuf,
};

#[derive(Parser, Debug)]
#[command(name = "neruda")]
#[command(version, about = "A work-in-progress compiler", long_about = None)]
struct Cli {
    #[arg(value_name = "FILE")]
    input: PathBuf,

    #[arg(short, long, value_name = "OUTPUT")]
    output: Option<PathBuf>,

    #[arg(short, long, value_enum, default_value_t = EmitTarget::Ast)]
    emit: EmitTarget,
}

#[derive(Copy, Clone, PartialEq, Eq, PartialOrd, Ord, ValueEnum, Debug)]
enum EmitTarget {
    Ast,
    Ir,
    Asm,
    Bin,
}

fn main() {
    let cli = Cli::parse();

    match cli.emit {
        EmitTarget::Ast => {
            let mut buf = String::new();
            File::open(&cli.input)
                .expect("Unable to open input file")
                .read_to_string(&mut buf)
                .expect("Unable to read input file");
            let parser = gen_parser();
            let tokens = match parser.lexer.lex_utf8(&buf) {
                Ok(tokens) => tokens,
                Err(e) => {
                    return e
                        .print(&buf, cli.input.to_str())
                        .expect("Unable to print lexing err");
                }
            };
            let ast = match parser.parse(&tokens, &buf) {
                Ok(tokens) => tokens,
                Err(e) => {
                    return e
                        .print(&buf, cli.input.to_str())
                        .expect("Unable to print parsing err");
                }
            };
            let ModuleOk {
                module,
                diagnostics,
            } = lowering::module_named("main.nrd", &buf, ast.entry).expect("somting went wrong :)");

            for warn in diagnostics.warns {
                println!("Warning: {} - {:?}", warn.inner, warn.location)
            }

            if let Some(out) = &cli.output {
                let mut buf = Vec::new();
                write!(buf, "{module:?}").expect("Insufficent space");
                let mut file = File::create(out).expect("Unable to open output file");
                file.write_all(&buf)
                    .expect("Unable to write all to output file");
            }
        }
        EmitTarget::Ir => println!("Running front-end... generating IR."),
        EmitTarget::Asm => println!("Running back-end... generating Assembly."),
        EmitTarget::Bin => println!("Full pipeline... generating Binary."),
    }
}
