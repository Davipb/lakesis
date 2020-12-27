use crate::core::{Error, VoidResult};
use std::env;
use std::fs::File;
use std::io::{Cursor, Read};
use std::path::{self, Path};

mod assembler;
mod core;
mod opcodes;

fn main() -> VoidResult {
    let raw_args: Vec<String> = env::args().collect();
    let verb: &str = &raw_args.get(1).map(|x| x as &str).unwrap_or("");

    let args = if raw_args.len() >= 3 {
        &raw_args[2..]
    } else {
        &[]
    };

    match verb {
        "view" => disassemble(args),
        "asm" => assemble(args),
        _ => print_help(&raw_args),
    }
}

fn print_help(args: &[String]) -> VoidResult {
    let program_name = args.get(0).map(|x| x as &str).unwrap_or("lakesis");

    println!("{} help", program_name);
    println!("\tPrints this message");
    println!();

    println!("{} asm <source> [output]", program_name);
    println!("\tCompiles an assembly source code file to an executable");
    println!("\tsource: Path of the file containing the assembly source code");
    println!("\toutput: Path of the file where the executable will be written to.");
    println!("\t        If not specified, uses the same file as 'source' but with a");
    println!("\t        .bin extension");
    println!();

    println!("{} view <file>", program_name);
    println!("\tDisassembles an executable and displays its code");
    println!("\tfile: Path of the file to disassemble");
    println!();

    Ok(())
}

fn disassemble(args: &[String]) -> VoidResult {
    if args.len() != 1 {
        return Err(Error::new("Expected exactly 1 argument"));
    }

    let mut file = File::open(&args[0])?;
    let mut buffer = Vec::with_capacity(file.metadata()?.len() as usize);
    file.read_to_end(&mut buffer)?;
    let buffer_size = buffer.len();

    let mut cursor = Cursor::new(buffer);

    while (cursor.position() as usize) < buffer_size {
        let opcode = opcodes::Opcode::decode(&mut cursor)?;
        println!("{:016X} {}", cursor.position(), opcode);
    }

    Ok(())
}

fn assemble(args: &[String]) -> VoidResult {
    if args.len() < 1 || args.len() > 2 {
        return Err(Error::new("Expected 1 or 2 arguments"));
    }

    let source_path = Path::new(&args[0]);
    let result_path = if args.len() >= 2 {
        Path::new(&args[1]).to_owned()
    } else {
        source_path.with_extension("bin")
    };

    let mut source = File::open(source_path)?;
    let mut result = File::create(result_path)?;

    assembler::assemble(&mut source, &mut result)?;
    Ok(())
}
