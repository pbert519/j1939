use j1939::{self, name::Name, time::Timer};

pub struct StdTimer(std::time::Instant);

impl StdTimer {
    pub fn new() -> Self {
        Self(std::time::Instant::now())
    }
}

impl j1939::time::TimerDriver for StdTimer {
    fn now(&self) -> u64 {
        let duration = self.0.elapsed();
        duration.as_millis() as u64
    }
}

fn main() {
    // create a socket and set receive timeout to 10ms
    let socket = candev::Socket::new("vcan0").expect("Could not open socketcan interface");
    socket.set_nonblocking(true).unwrap();

    Timer::init(Box::new(StdTimer::new()));
    let mut stack = j1939::stack::Stack::new(socket);

    let _cf = stack.register_control_function(0x80, Name::default());

    loop {
        stack.process();
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
}
