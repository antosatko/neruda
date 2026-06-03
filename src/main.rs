use clap::{Parser, ValueEnum};
use ir::const_stage::{Context, objects::Objects, types::Types};
use parser::{
    grammar::gen_parser,
    lowering::{self, ModuleOk},
    parse_directory,
};
use std::{
    collections::HashMap,
    fs::File,
    io::{Read, Write},
    path::PathBuf,
    sync::Arc,
};

#[derive(Parser, Debug)]
#[command(name = "neruda")]
#[command(version, about = "A work-in-progress compiler", long_about = None)]
struct Cli {
    #[arg(value_name = "FILE")]
    input: PathBuf,

    #[arg(short, long, value_name = "OUTPUT")]
    output: Option<PathBuf>,

    #[arg(short, long, value_enum, default_value_t = EmitTarget::Bin)]
    emit: EmitTarget,
}

#[derive(Copy, Clone, PartialEq, Eq, PartialOrd, Ord, ValueEnum, Debug)]
enum EmitTarget {
    Ast,
    AstPretty,
    Ir,
    Asm,
    Bin,
}

fn main() {
    let cli = Cli::parse();

    match cli.emit {
        EmitTarget::Ast | EmitTarget::AstPretty => {
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
                        .print(&buf, Some(&cli.input))
                        .expect("Unable to print lexing err");
                }
            };
            let ast = match parser.parse(&tokens, &buf) {
                Ok(tokens) => tokens,
                Err(e) => {
                    return e
                        .print(&buf, Some(&cli.input))
                        .expect("Unable to print parsing err");
                }
            };
            let ModuleOk {
                module,
                diagnostics,
            } = lowering::module_named(
                "main.nrd",
                &buf,
                ast.entry,
                Some(PathBuf::from("./main.nrd")),
            )
            .expect("somting went wrong :)");

            for warn in diagnostics.warns {
                println!("Warning: {} - {:?}", warn.inner, warn.location)
            }

            if let Some(out) = &cli.output {
                let mut buf = Vec::new();
                match cli.emit {
                    EmitTarget::Ast => writeln!(buf, "{module:?}").expect("Insufficent space"),
                    EmitTarget::AstPretty => {
                        writeln!(buf, "{module:#?}").expect("Insufficent space")
                    }
                    _ => unreachable!(),
                };
                let mut file = File::create(out).expect("Unable to open output file");
                file.write_all(&buf)
                    .expect("Unable to write all to output file");
            }
        }
        EmitTarget::Ir => {
            let modules = parse_directory(&cli.input, None, |str, path, e| {
                e.print(str, Some(path)).unwrap();
            })
            .unwrap();
            for ModuleOk {
                module: _,
                diagnostics,
            } in modules.values()
            {
                for warn in &diagnostics.warns {
                    println!("Warning: {} - {:?}", warn.inner, warn.location)
                }
            }

            let ir_ctx = Context::from_ast(HashMap::from_iter(
                modules
                    .iter()
                    .map(|(key, mok)| (key.clone(), Arc::new(mok.module.clone()))),
            ));

            let ir_ctx = match ir_ctx {
                Ok(c) => c,
                Err((ir_ctx, err)) => {
                    err.print(&ir_ctx).unwrap();
                    return;
                }
            };

            for warn in &ir_ctx.diagnostics.warnings {
                warn.print(&ir_ctx).unwrap();
            }

            {
                let Types {
                    functions,
                    constraints,
                    structures,
                    arrays,
                    tuples,
                    modules,
                    named,
                    traits,
                    enums,
                    references,
                    morphs,
                    generics: _,
                    polymorphs,
                } = &ir_ctx.types;
                println!("function types:");
                for t in functions.iter() {
                    println!("\t{}", t.stringify(&ir_ctx.types));
                }
                println!("constraint types:");
                for t in constraints.iter() {
                    println!("\t{}", t.stringify(&ir_ctx.types));
                }
                println!("structure types:");
                for t in structures.iter() {
                    println!("\t{}", t.stringify(&ir_ctx.types));
                }
                println!("enum types:");
                for t in enums.iter() {
                    println!("\t- {}", t.stringify(&ir_ctx.types));
                }
                println!("array types:");
                for t in arrays.iter() {
                    println!("\t{}", t.stringify(&ir_ctx.types));
                }
                println!("tuple types:");
                for t in tuples.iter() {
                    println!("\t{}", t.stringify(&ir_ctx.types));
                }
                println!("trait types:");
                for t in traits.iter() {
                    println!("\t{}", t.stringify());
                }
                println!("reference types:");
                for t in references.iter() {
                    println!("\t{}", t.inner.stringify(&ir_ctx.types));
                }
                println!("module refs:");
                for t in modules.iter() {
                    println!("\t- {}", t.stringify());
                }
                println!("morphed:");
                for t in morphs.iter() {
                    println!("\t- {}", t.stringify(&ir_ctx.types));
                }
                println!("polymorphs:");
                for t in polymorphs.iter() {
                    println!("\t- {}", t.stringify(&ir_ctx.types));
                }
                println!("named types:");
                for t in named.iter() {
                    println!("\t- {}", t.stringify(&ir_ctx.types));
                }
            }
            {
                let Objects {
                    imports,
                    constants,
                    types,
                    traits,
                    components,
                    functions,
                    resources,
                } = &ir_ctx.objects;
                for fun in functions.iter() {
                    println!("Function: {}", fun.identifier);
                    fun.data.ir.get_done().variables.iter().for_each(|v| {
                        println!("\tvar {}: {}", *v.identifier, v.ty.stringify(&ir_ctx.types))
                    });
                }
            }
        }
        EmitTarget::Asm => println!("Running back-end... generating Assembly."),
        EmitTarget::Bin => println!("Full pipeline... generating Binary."),
    }
}
