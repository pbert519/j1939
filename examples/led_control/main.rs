mod display;
mod ecu;
mod messages;

use display::*;
use ecu::*;
use j1939::{self, time::std::StdTimerDriver};
use socketcan::{CanSocket, Socket};

fn main() {
    // create a socket and set to non blocking
    let socket = CanSocket::open("vcan0").unwrap();
    socket.set_nonblocking(true).unwrap();
    let mut stack = j1939::stack::Stack::new(socket, StdTimerDriver::new());

    let mut display = Display::new(&mut stack);
    let mut ecu = ECU::new(&mut stack);

    loop {
        // running the j1939 stack does the actually heavy lifting including transport protocols and address management
        stack.process();

        // our ecu logic
        // the ecu provides values for different parameters
        // additionally is possible to set parameter within the ecu
        ecu.process(&mut stack);

        // our consumer logic
        // reads and write parameters of the ecu
        display.process(&mut stack);

        std::thread::sleep(std::time::Duration::from_millis(10));
    }
}
