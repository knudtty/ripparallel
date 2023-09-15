use criterion::{black_box, criterion_group, criterion_main, Criterion};
use ripparallel::shell::Shell;
use std::fs::File;
use std::fs::OpenOptions;
use std::io::prelude::*;
use std::io::{Read, Write};
use std::process::{Child, ChildStderr, ChildStdin, ChildStdout, Command, Stdio};
use tempfile::tempfile;

pub struct BenchShell {
    #[allow(dead_code)]
    slave: Child,
    stdin: ChildStdin,
    stdout: ChildStdout,
    stderr: ChildStderr,
    file: File,
    file_size: usize,
}

pub struct StdOut(pub Vec<u8>);
pub struct StdErr(pub Option<Vec<u8>>);
type Execution = (StdOut, StdErr);
impl BenchShell {
    pub fn kill(&mut self) {
        self.stdin
            .write(b"exit\n")
            .expect("Failed to kill shell process");
        self.slave.wait().expect("Failed to kill process");
    }

    pub fn new_with_bash_func(bash_cmd: &str, n: usize) -> Self {
        return Self::birth(None, bash_cmd, n);
    }
    fn birth(shell_name: Option<&str>, bash_arg: &str, n: usize) -> Self {
        // Spawn a new shell process
        let mut child = Command::new(shell_name.unwrap_or("sh"))
            .arg("-c")
            .arg(bash_arg)
            .stdout(Stdio::piped())
            .stdin(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .expect("Failed to spawn shell process");

        let child_stdin = child.stdin.take().expect("Failed to open stdin");
        let child_stdout = child.stdout.take().expect("Failed to open stdout");
        let child_stderr = child.stderr.take().expect("Failed to open stdout");
        let mut file = OpenOptions::new()
            .create_new(true)
            //.read(true)
            .open(format!("/tmp/bingotime{}", n))
            .unwrap();

        BenchShell {
            slave: child,
            stdin: child_stdin,
            stdout: child_stdout,
            stderr: child_stderr,
            file,
            file_size: 0,
        }
    }

    pub fn feed(&mut self, command: &str) {
        self.stdin
            .write_all(command.as_bytes())
            .expect("Failed to write to stdin");
        self.stdin
            .write_all(b"\n")
            .expect("Failed to write newline to stdin");
    }

    //fn harass(&mut self) -> Execution {
    //    let mut stdout: Vec<u8> = Vec::new();
    //    let mut stderr: Vec<u8> = Vec::new();
    //    loop {
    //        let mut buf = [0; BUF_SIZE];
    //        match self.stdout.read(&mut buf) {
    //            Ok(0) => {
    //                if stdout.len() > 0 {
    //                    break;
    //                }
    //            }
    //            Ok(n_bytes_read) => {
    //                stdout.extend(buf.iter().take(n_bytes_read));
    //                let last_byte = &buf[n_bytes_read - 1];
    //                // TODO: Fix this bug. Shouldn't exit anytime it finds 4 at the end of the
    //                // buffer
    //                if WANT_BYTE == *last_byte && n_bytes_read < BUF_SIZE {
    //                    break;
    //                }
    //            }
    //            Err(_) => {
    //                eprintln!("ERROR");
    //            }
    //        }
    //    }
    //    loop {
    //        let mut buf = [0; BUF_SIZE];
    //        match self.stderr.read(&mut buf) {
    //            Ok(0) => {
    //                if stderr.len() > 0 {
    //                    break;
    //                }
    //            }
    //            Ok(n_bytes_read) => {
    //                stderr.extend(buf.iter().take(n_bytes_read));
    //                let last_byte = &buf[n_bytes_read - 1];
    //                if WANT_BYTE == *last_byte {
    //                    break;
    //                }
    //            }
    //            Err(_) => {
    //                eprintln!("ERROR");
    //            }
    //        }
    //    }
    //    // only 1 unit
    //    if stderr.len() == 1 {
    //        return (StdOut(stdout), StdErr(None));
    //    }
    //    let stderr: Vec<_> = Vec::new();
    //    return (StdOut(stdout), StdErr(Some(stderr)));
    //}
}

fn command_spawn(n: usize) {
    use std::fmt::Write;

    // Read and print the output
    let mut output = String::new();
    let mut parent_stdout = String::new();
    // Spawn a new shell process
    let mut child = Command::new("echo")
        .arg(n.to_string())
        .stdout(Stdio::piped())
        .spawn()
        .expect("Failed to spawn shell process");
    child.wait().expect("Failed to wait for child process");
    child
        .stdout
        .unwrap()
        .read_to_string(&mut output)
        .expect("failed to read to string");
    write!(parent_stdout, "{}", output).expect("Failed to write to parent stdout");
}

fn command_shell(shell: &mut Shell, n: usize) {
    // Spawn a new shell process
    shell.execute(format!("echo {}", n.to_string()));
}

fn tmp_file(shell: &mut BenchShell, file_size: &mut usize, n: usize) {
    shell.feed("echo 1");
    let mut buf = [0; 50];
    shell.stdout.read(&mut buf).expect("failed to read");
    shell.file_size = shell.file.metadata().unwrap().len() as usize;
    println!("File size: {}", shell.file_size);
}

fn criterion_benchmark(c: &mut Criterion) {
    let n: usize = 1;
    //c.bench_function("command_spawn", |b| b.iter(|| command_spawn(black_box(n))));

    //c.bench_function("command_shell", |b| {
    //    let mut shell = Shell::new();
    //    b.iter(|| command_shell(&mut shell, black_box(n)));
    //    shell.kill();
    //});

    //c.bench_function("zsh shell", |b| {
    //    let mut shell = Shell::new_with("zsh");
    //    b.iter(|| command_shell(&mut shell, black_box(n)));
    //    shell.kill();
    //});

    //c.bench_function("bash shell", |b| {
    //    let mut shell = Shell::new_with("bash");
    //    b.iter(|| command_shell(&mut shell, black_box(n)));
    //    shell.kill();
    //});

    //c.bench_function("dash shell", |b| {
    //    let mut shell = Shell::new_with("dash");
    //    b.iter(|| command_shell(&mut shell, black_box(n)));
    //    shell.kill();
    //});

    c.bench_function("tmp file size", |b| {
        let bashfunc = format!(
            "while true; do 
            read line; 
            if [ \"$line\" = \"exit\" ]; then 
                exit; 
            fi; 
            ${{line}}; 
            printf \"\x01\" > /tmp/bingotime{}; 
            done;
        ",
            n
        );
        let mut shell = BenchShell::new_with_bash_func(bashfunc.as_str(), n);
        let mut file_size = 1;
        b.iter(|| tmp_file(&mut shell, &mut file_size, black_box(n)));
    });
}

criterion_group!(benches, criterion_benchmark);
criterion_main!(benches);
