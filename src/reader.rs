use crate::shell::RAND_STRING_SIZE;
use crate::jobs::JobOut;

struct SuperSpecialReader {
    job: JobOut,
    buf: Vec<u8>,
    end_indication_bytes: [u8; RAND_STRING_SIZE],
    byte_history: [u8; RAND_STRING_SIZE],
    uncleared_message: Vec<u8>,
    cached_message: Vec<u8>,
}

impl SuperSpecialReader {
    //fn update_job();
}
