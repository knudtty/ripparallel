use clap::Parser;
use crossbeam_channel::{unbounded, Sender};
use ripparallel::shell::{JobResult, Shell};
use ripparallel::thread_pool;
use std::io::{self, Write};
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

fn parse_command(command_args: Arc<Vec<String>>, job_input: String) -> String {
    let cmd_args_len = command_args.len();
    let cmditer = command_args.iter().enumerate();
    let mut command = String::with_capacity(
        command_args
            .iter()
            .map(|arg| arg.len() + 1 /* +1 for space */)
            .sum::<usize>()
            + job_input.len()
            + 10,
    );
    let mut substituted = false;
    cmditer.for_each(|(i, arg)| {
        if arg.as_str() == "{}" {
            substituted = true;
            command.push_str(job_input.as_str());
            command.push(' ');
        } else {
            command.push_str(arg.as_str());
            // not end of command, should push a space
            if i != cmd_args_len - 1 {
                command.push(' ');
            // end of command, should push a space if substitution is
            // expected to take place at end
            } else if !substituted {
                command.push(' ');
            }
        }
    });
    if !substituted {
        command.push_str(job_input.as_str());
    }
    command
}
fn main() {
    let args = Args::parse();
    let input = io::stdin();

    let jobs = args.jobs.unwrap_or(thread_pool::max_par() - 1);
    let command = Arc::new(args.job);

    let channel_size = jobs;
    let (job_sender, job_receiver) = unbounded();
    let (stdout_sender, stdout_receiver) = unbounded();

    let handles: Vec<JoinHandle<Result<(), ()>>> = (0..jobs)
        .map(|_| {
            let stdout_sender: Sender<(usize, JobResult)> = stdout_sender.clone();
            let command_args = Arc::clone(&command);
            let job_receiver = job_receiver.clone();
            return thread::spawn(move || -> Result<(), ()> {
                let mut shell = Shell::new(stdout_sender);
                // single receiver shared between all threads
                for job in job_receiver {
                    match job {
                        Message::Job((i, job_input)) => {
                            let command_args = command_args.clone();
                            let command = parse_command(command_args, job_input);
                            //eprintln!("id of job: {}", i);
                            shell.execute(command, i);
                        }
                        Message::Quit => {
                            shell.kill();
                            break;
                        }
                    }
                }
                Ok(())
            });
        })
        .collect();

    // all steps necessary in order to preparing to start
    drop(stdout_sender);
    let mut lines_iter = input
        .lines()
        .map(|res| res.expect("Error reading line from stdin"))
        .enumerate()
        .peekable();
    // fill jobs initially
    for _ in 0..channel_size {
        let (i, line_res) = lines_iter.next().unwrap();
        job_sender
            .send(Message::Job((i, line_res)))
            .expect("Error sending job to thread");
    }

    // pseudocode for streaming back
    // if number is next in line what we expect:
    //      if partial job result:
    //          write nbytes to stdout
    //      else if complete:
    //          write nbytes to stdout and advance next_customer
    //
    // if number is not next in line:
    //      find entry in container (via job_id key) for vec and push additional JobResult to it
    //
    // if we received a completion and the number is what we expect:
    //
    // TODO: Make this ordering thing into a function
    let mut next_customer = 0;
    let mut waiting_room: Vec<(usize, Vec<JobResult>)> = Vec::new();
    let mut stdout = std::io::stdout();
    for (i, new_output) in stdout_receiver {
        //eprintln!("new line");
        // before calling waiting room function
        if lines_iter.peek().is_some() {
            let n_messages_under = channel_size - job_sender.len();
            //eprintln!("messages under: {}", n_messages_under);
            for _ in 0..n_messages_under {
                match lines_iter.next() {
                    Some((id, job)) => {
                        job_sender
                            .send(Message::Job((id, job)))
                            .expect("Broker: Unable to send job");
                    }
                    _ => {
                        break;
                    }
                }
            }
        } else {
            job_sender
                .send(Message::Quit)
                .expect("Unable to send quit message");
        }
        // before calling waiting room function
        if i == next_customer {
            // portable function part
            // increment next_customer and write to stdout
            match new_output {
                JobResult::Partial(res) => {
                    stdout
                        .write(&res.bytes[0..res.n_bytes])
                        .expect("Failed to write to stdout");
                }
                JobResult::Completion(res) => {
                    stdout
                        .write(&res.bytes[0..res.n_bytes])
                        .expect("Failed to write to stdout");
                    next_customer += 1;
                }
            }
            loop {
                // check for next customer in waiting_room
                if let Some(idx) = waiting_room
                    .iter()
                    .position(|(customer, _)| customer == &next_customer)
                {
                    // if found, copy entry to new_customer, copy last entry into indexed spot and pop last entry.
                    let v = waiting_room.swap_remove(idx).1;
                    v.iter().for_each(|job_res| match job_res {
                        JobResult::Partial(res) => {
                            stdout
                                .write(&res.bytes[0..res.n_bytes])
                                .expect("Failed to write to stdout");
                        }
                        JobResult::Completion(res) => {
                            stdout
                                .write(&res.bytes[0..res.n_bytes])
                                .expect("Failed to write to stdout");
                            next_customer += 1;
                        }
                    });
                } else {
                    break;
                }
            }
        } else {
            if let Some(idx) = waiting_room
                .iter()
                .position(|(customer, _)| customer == &next_customer)
            {
                waiting_room.get_mut(idx).unwrap().1.push(new_output);
            } else {
                //eprintln!("pushing");
                let mut v = Vec::new();
                v.push(new_output);
                waiting_room.push((i, v));
            }
        }
        //eprintln!("i is {}", i);
    }

    // Send lines to the channel from the main thread
    for handle in handles {
        let _ = handle.join();
    }
}
