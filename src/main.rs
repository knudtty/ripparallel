use clap::Parser;
use ripparallel::thread_pool;
use once_cell::unsync;
use std::io::{self, Write};
use std::process::Command;

#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None, trailing_var_arg = true)]
struct Args {
    /// Number of jobs
    #[arg(short, long)]
    jobs: Option<usize>,

    /// Placeholder
    #[arg(short = 'I')]
    replace_string: Option<String>,

    /// job to run
    job: Vec<String>,
}

fn the_thing(line: &'static String, command: &Vec<String>) -> Vec<u8> {
    let proper_command: String = command.iter().fold(String::new(), |mut s, c| {
        s.push_str(" ");
        s.push_str(match c.as_str() {
            "{}" => line,
            _ => c,
        });
        s
    });
    let res = Command::new("sh").arg("-c").arg(&proper_command).output().expect("deez nuts broken");
    res.stdout
}

static mut INPUTS: unsync::Lazy<Vec<String>> = unsync::Lazy::new(|| Vec::new());

fn main() {
    let mut args = Args::parse();
    let input = io::stdin();

    // read from stdin
    input
        .lines()
        .for_each(|l| unsafe { INPUTS.push(l.unwrap()) });
    if !args.job.iter().any(|v| v.as_str()=="{}") {
        args.job.push(String::from("{}"));
    }

    // start threads to run the function
    let threads = thread_pool::do_the_thing(
        args.jobs.unwrap_or(thread_pool::max_par()),
        unsync::Lazy::get(unsafe { &INPUTS }).unwrap(),
        &the_thing,
        args.job,
    );

    // collect threads to print to stdout
    for thread in threads {
        let res = thread.join().expect("Thread failed");

        res.iter().for_each(|output| {
            io::stdout()
                .write(output.as_slice())
                .expect("can't write to stdout");
        })
    }
}
