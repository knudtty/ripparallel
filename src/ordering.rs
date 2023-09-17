pub struct WaitingRoom<T, F>
where
    F: Fn(T),
{
    next_customer: usize,
    waiting_room: Vec<(usize, T)>,
    f: F, //do_function:
}

impl<T, F> WaitingRoom<T, F>
where
    F: Fn(T),
{
    pub fn new(f: F) -> Self {
        WaitingRoom {
            next_customer: 0,
            waiting_room: Vec::new(),
            f,
        }
    }
    pub fn serve_customer(&mut self, i: usize, mut customer: T) {
        // before calling waiting room function
        if i == self.next_customer {
            loop {
                // call function to do something once customer is found
                (self.f)(customer);
                // check for next customer in waiting_room
                self.next_customer += 1;
                if let Some(idx) = self
                    .waiting_room
                    .iter()
                    .position(|(i, _)| i == &self.next_customer)
                {
                    customer = self.waiting_room.swap_remove(idx).1;
                } else {
                    break;
                }
            }
        } else {
            self.waiting_room.push((i, customer));
        }
    }
}
