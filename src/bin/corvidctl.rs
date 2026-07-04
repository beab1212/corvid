//! `corvidctl` — the corvid command-line driver.
//!
//! Subcommands:
//! * `version`               — print the build version.
//! * `inspect <file>`        — dump the structure of a CVWP stream.
//! * `run <capture>`         — replay a capture through the pipeline.
//! * `filter <expr>`         — parse and echo a filter expression's cost.
//! * `asm <file>`            — assemble VM source to bytecode (hex).
//! * `snapshot <file>`       — decode and summarise a flow snapshot.
//! * `validate <file>`       — inspect + report on a stream's schemas.
//!
//! It is deliberately argument-parser-free to keep the binary small.

use std::process::ExitCode;

use corvid::config::Config;
use corvid::error::Result;
use corvid::export::Format;

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().collect();
    let cmd = match args.get(1).map(String::as_str) {
        Some(c) => c,
        None => {
            usage();
            return ExitCode::from(2);
        }
    };

    let result = match cmd {
        "version" => {
            println!("corvid {}", corvid::VERSION);
            Ok(())
        }
        "inspect" => cmd_inspect(args.get(2)),
        "run" => cmd_run(args.get(2), args.get(3)),
        "filter" => cmd_filter(&args[2..]),
        "asm" => cmd_asm(args.get(2)),
        "snapshot" => cmd_snapshot(args.get(2)),
        "validate" => cmd_validate(args.get(2)),
        "help" | "-h" | "--help" => {
            usage();
            Ok(())
        }
        other => {
            eprintln!("unknown subcommand: {other}");
            usage();
            return ExitCode::from(2);
        }
    };

    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("error: {e}");
            ExitCode::FAILURE
        }
    }
}

fn usage() {
    eprintln!(
        "usage: corvidctl <command> [args]\n\
         \n\
         commands:\n  \
           version                 print version\n  \
           inspect <file>          dump a CVWP stream's structure\n  \
           run <capture> [filter]  replay a capture through the pipeline\n  \
           filter <expr...>        parse a filter expression\n  \
           asm <file>              assemble VM source to bytecode\n  \
           snapshot <file>         summarise a flow snapshot\n  \
           validate <file>         report on a stream's schemas"
    );
}

fn read_file(path: Option<&String>) -> Result<Vec<u8>> {
    let path = path.ok_or_else(|| corvid::Error::malformed("missing file argument"))?;
    Ok(std::fs::read(path)?)
}

fn cmd_inspect(path: Option<&String>) -> Result<()> {
    let bytes = read_file(path)?;
    let lines = corvid::inspect::inspect_stream(&bytes)?;
    print!("{}", corvid::inspect::render(&lines));
    println!("{} message(s)", lines.len());
    Ok(())
}

fn cmd_run(path: Option<&String>, filter: Option<&String>) -> Result<()> {
    let bytes = read_file(path)?;
    let mut pipeline = corvid::pipeline::Pipeline::new(Config::default(), Format::Text);
    if let Some(expr) = filter {
        pipeline = pipeline.with_filter(expr)?;
    }
    // Try as a capture file first; fall back to a single raw stream.
    if bytes.len() >= 4 && &bytes[0..4] == b"CVCP" {
        pipeline.feed_capture(&bytes)?;
    } else {
        pipeline.feed_stream(1, &bytes)?;
    }
    println!("{}", pipeline.report());
    println!("streams_in={} matched={}", pipeline.streams_in(), pipeline.matched());
    Ok(())
}

fn cmd_filter(parts: &[String]) -> Result<()> {
    if parts.is_empty() {
        return Err(corvid::Error::malformed("filter: expression required"));
    }
    let expr = parts.join(" ");
    let filter = corvid::filter::Filter::compile(&expr)?;
    println!("ok: {} field reference(s)", filter.cost());
    Ok(())
}

fn cmd_asm(path: Option<&String>) -> Result<()> {
    let bytes = read_file(path)?;
    let src = std::str::from_utf8(&bytes).map_err(|_| corvid::Error::malformed("asm: not utf8"))?;
    let code = corvid::engine::assembler::assemble(src)?;
    let hex: Vec<String> = code.iter().map(|b| format!("{b:02x}")).collect();
    println!("{}", hex.join(" "));
    println!("{} byte(s)", code.len());
    Ok(())
}

fn cmd_snapshot(path: Option<&String>) -> Result<()> {
    let bytes = read_file(path)?;
    let summary = corvid::snapshot::replay_into_summary(&bytes, 16)?;
    print!("{}", summary.report());
    Ok(())
}

fn cmd_validate(path: Option<&String>) -> Result<()> {
    let bytes = read_file(path)?;
    let lines = corvid::inspect::inspect_stream(&bytes)?;
    let mut schema_count = 0;
    for l in &lines {
        if matches!(
            l.ty,
            corvid::wire::MsgType::SchemaDef | corvid::wire::MsgType::SchemaUpdate
        ) {
            schema_count += 1;
        }
    }
    println!("messages={} schemas={}", lines.len(), schema_count);
    Ok(())
}
