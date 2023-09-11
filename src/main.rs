use clap::Parser;
use std::io::{self, Write};
use std::process::Command;
use std::sync::Arc;
use rayon::prelude::*;

#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None, trailing_var_arg = true)]
struct Args {
    /// Number of jobs
    #[arg(short, long)]
    jobs: Option<usize>,

    /// Placeholder
    #[arg(short = 'I')]
    replace_string: Option<String>,

    /// Maintain order that inputs came in
    #[arg(short, long)]
    order: Option<bool>,

    /// job to run
    job: Vec<String>,
}

type JobResult = Vec<u8>;
type JobInput = String;

fn the_thing(line: &String, command: &Arc<Vec<String>>) -> JobResult {
    let mut cmditer = command.iter();
    let mut cmd: Command;
    let mut substituted = false;
    match cmditer.next().expect("shid broken").as_ref() {
        "{}" => {
            substituted = true;
            cmd = Command::new("sh");
            cmd.arg("-c").arg(line.as_str());
        }
        first => cmd = Command::new(first),
    };
    cmditer.for_each(|arg| {
        if arg.as_str() == "{}" {
            substituted = true;
            cmd.arg(line.as_str());
        } else {
            cmd.arg(arg);
        }
    });
    if !substituted {
        cmd.arg(line.as_str());
    };
    let res = cmd.output().expect("deez nuts broken");
    res.stdout.into()
}

fn main() {
    let args = Args::parse();
    let input = io::stdin();

    let command = Arc::new(args.job);

    let input: Vec<JobInput> = input.lines().map(|v| v.unwrap()).collect();
    input.par_iter().enumerate().for_each(|(_, line)| {
        let thing_to_write = the_thing(line, &command);
        io::stdout().lock().write_all(&thing_to_write).unwrap();
    });
}
