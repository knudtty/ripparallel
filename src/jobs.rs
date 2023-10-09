use std::fs::File;

pub enum Message {
    Job((usize, String)),
    Quit,
}

#[derive(Debug)]
pub enum JobOut {
    File(File),
    Memory(Vec<u8>),
    None,
}

