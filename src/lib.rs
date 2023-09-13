pub mod shell;
pub mod thread_pool {
    use std::sync::Arc;
    use std::thread::{self, JoinHandle};

    pub fn do_the_thing<F>(
        jobs: usize,
        lines: &'static Vec<String>,
        f: &'static F,
        job: Vec<String>,
    ) -> Vec<JoinHandle<Vec<Vec<u8>>>>
    where
        F: Fn(&'static String, &Vec<String>) -> Vec<u8> + Sync,
    {
        let n_lines = lines.len();
        let jobs = if jobs == 0 || jobs > n_lines {
            n_lines
        } else {
            jobs
        };
        let ajob = Arc::new(job);
        lines
            .chunks(n_lines / jobs)
            .map(|chunk| {
                let bjob = Arc::clone(&ajob);
                thread::spawn(move || chunk.iter().map(|elem| f(elem, &bjob)).collect())
            })
            .collect()
    }

    pub fn max_par() -> usize {
        return thread::available_parallelism().unwrap().get();
    }
}
