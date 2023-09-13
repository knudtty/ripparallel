use clap::Parser;
use crossbeam_channel::bounded;
use ripparallel::thread_pool;
use std::io;
use std::process::Stdio;
use std::process::{ChildStdout, Command};
use std::sync::Arc;
use std::thread::{self, JoinHandle};

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

enum Message {
    Job((usize, String)),
    Quit,
}

type JobResult = ChildStdout;

fn the_thing(line: String, command: &Arc<Vec<String>>) -> Result<JobResult, ()> {
    // TODO: Construct command without sh -c
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
    let mut child = cmd
        .stdout(Stdio::piped())
        .spawn()
        .expect("deez nuts broken");
    child.wait().expect("error during child process execution");
    return Ok(child.stdout.expect("No stdout"));
}

fn main() {
    let args = Args::parse();
    let input = io::stdin();

    let jobs = args.jobs.unwrap_or(thread_pool::max_par() - 1);
    let command = Arc::new(args.job);

    let (job_sender, job_receiver) = bounded(jobs);
    let (stdout_sender, stdout_receiver) = bounded(jobs);

    let handles: Vec<JoinHandle<Result<(), ()>>> = (0..jobs)
        .map(|_| {
            let stdout_sender = stdout_sender.clone();
            let this_command = Arc::clone(&command);
            let job_receiver = job_receiver.clone();
            return thread::spawn(move || -> Result<(), ()> {
                // single receiver shared between all threads
                for job in job_receiver {
                    match job {
                        Message::Job((i, job_input)) => {
                            let res = the_thing(job_input, &this_command);
                            stdout_sender.send((i, res?)).expect("hey");
                        }
                        Message::Quit => break,
                    }
                }
                Ok(())
            });
        })
        .collect();

    // all steps necessary in order to preparing to start
    drop(stdout_sender);
    let mut lines_iter = input.lines().map(|res| res.expect("Error reading line from stdin")).enumerate().peekable();
    // fill jobs initially
    for _ in 0..jobs {
        let (i, line_res) = unsafe { lines_iter.next().unwrap_unchecked() };
        job_sender
            .send(Message::Job((i, line_res)))
            .expect("Error sending job to thread");
    }

    // TODO: Make this ordering thing into a function
    let mut next_customer = 0;
    let mut waiting_room: Vec<(usize, JobResult)> = Vec::new();
    let mut stdout = std::io::stdout();
    for (i, output) in stdout_receiver {
        // before calling waiting room function
        if lines_iter.peek().is_some() {
            let n_messages_under = jobs - job_sender.len();
            for _ in 0..n_messages_under {
                match lines_iter.next() {
                    Some((id, job)) => {
                        job_sender
                            .send(Message::Job((id, job)))
                            .expect("Broker: Unable to send job");
                    }
                    _ => {}
                }
                break;
            }
        } else {
            job_sender
                .send(Message::Quit)
                .expect("Unable to send quit message");
        }
        // before calling waiting room function
        if i == next_customer {
            let mut new_customer = output;
            loop {
                // portable function part
                // increment next_customer and write to stdout
                io::copy(&mut new_customer, &mut stdout).expect("Writing to stdout failed");
                // portable function part
                next_customer += 1;
                // check for next customer in waiting_room
                if let Some(idx) = waiting_room
                    .iter()
                    .position(|(customer, _)| customer == &next_customer)
                {
                    // if found, copy entry to new_customer, copy last entry into indexed spot and pop last entry.
                    new_customer = waiting_room.swap_remove(idx).1;
                } else {
                    // else break loop
                    break;
                }
            }
        } else {
            waiting_room.push((i, output));
        }
    }

    // Send lines to the channel from the main thread
    for handle in handles {
        let _ = handle.join();
    }
}
