use core::time;
use embedded_hal::can::nb::Can;
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
    let mut socket = candev::Socket::new("vcan0").expect("Could not open socketcan interface");
    socket.set_nonblocking(false).unwrap();
    socket
        .set_read_timeout(time::Duration::from_millis(10))
        .unwrap();

    Timer::init(Box::new(StdTimer::new()));
    let mut stack = j1939::stack::Stack::new();

    let _cf = stack
        .register_control_function(0x80, Name::default())
        .unwrap();

    // three stage lib usage
    // stage 1: forward all frames into j1939 stack
    // stage 2: call process to run all interal stuff
    // stage 3: send all frames in the output buffer of the j1939 stack
    loop {
        while let Ok(frame) = socket.receive() {
            stack.push_can_frame(frame);
        }
        stack.process();
        while let Some(tx_frame) = stack.get_can_frame() {
            socket.transmit(&tx_frame).expect("Transmit error");
        }
    }
}
