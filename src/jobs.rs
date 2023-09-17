use std::fs::File;

pub enum Message {
    Job((usize, String)),
    Quit,
}

pub enum JobOut {
    File(File),
    Memory(Vec<u8>),
    None,
}

