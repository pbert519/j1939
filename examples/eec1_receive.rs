use j1939::{
    self,
    frame::{Frame, PGN},
};
use socketcan::{CanSocket, Socket};

const PGN_ELECTRONICENGINECONTROLLER: PGN = PGN(0xF004);
// Deserialized EEC1 Message but the fields have the raw values and the computation method is not applied!
#[derive(Debug)]
#[allow(dead_code)]
struct EEC1 {
    torque_mode: u8,
    additional_torque: u8,
    driver_demand_torque: u8,
    actual_torque: u8,
    engine_speed: u16,
    controller_sa: u8,
    starter_mode: u8,
    engine_demand_torque: u8,
}
impl From<Frame> for EEC1 {
    fn from(frame: Frame) -> Self {
        Self {
            torque_mode: frame.data()[0] & 0xF,
            additional_torque: (frame.data()[0] >> 4) & 0xF,
            driver_demand_torque: frame.data()[1],
            actual_torque: frame.data()[2],
            engine_speed: u16::from_le_bytes([frame.data()[3], frame.data()[4]]),
            controller_sa: frame.data()[5],
            starter_mode: frame.data()[6] & 0xF,
            engine_demand_torque: frame.data()[7],
        }
    }
}

fn main() {
    // create a socket and set to non blocking
    let socket = CanSocket::open("vcan0").unwrap();
    socket.set_nonblocking(true).unwrap();
    let mut stack = j1939::stack::Stack::new(socket, j1939::time::std::StdTimerDriver::new());

    loop {
        stack.process();
        // ToDo: Change to CF and use pgn filter
        while let Some(msg) = stack.get_frame() {
            if msg.header().pgn() == PGN_ELECTRONICENGINECONTROLLER {
                println!("{:?}", EEC1::from(msg));
            }
        }

        std::thread::sleep(std::time::Duration::from_millis(10));
    }
}
