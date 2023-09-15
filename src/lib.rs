pub mod shell;
pub mod thread_pool {
    use std::thread;

    pub fn max_par() -> usize {
        return thread::available_parallelism().unwrap().get();
    }
}
