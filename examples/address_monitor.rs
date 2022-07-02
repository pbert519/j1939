use j1939::{
    self,
    frame::{Request, PGN_ADDRESSCLAIM},
    time::Instant,
};

#[derive(Clone)]
pub struct StdTimer(std::time::Instant);

impl StdTimer {
    pub fn new() -> Self {
        Self(std::time::Instant::now())
    }
}

impl j1939::time::TimerDriver for StdTimer {
    fn now(&self) -> Instant {
        let duration = self.0.elapsed();
        Instant(duration.as_millis() as u64)
    }
}

fn main() {
    // create a socket and set to nonblocking
    let socket = candev::Socket::new("vcan0").expect("Could not open socketcan interface");
    socket.set_nonblocking(true).unwrap();
    let mut stack = j1939::stack::Stack::new(socket, StdTimer::new());

    let mut counter = 0;
    loop {
        stack.process();
        std::thread::sleep(std::time::Duration::from_millis(10));
        counter += 1;
        // run every 1s
        if counter > 100 {
            counter = 0;
            println!("{:#?}", stack.control_function_list());
            // Send a Address Request to update the address list
            let req = Request::new(PGN_ADDRESSCLAIM, 0xFE, 0xFF);
            stack.send_frame(req.into());
        }
    }
}
