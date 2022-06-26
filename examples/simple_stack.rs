use j1939::{self, name::*, time::Instant};

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
    // create a socket and set receive timeout to 10ms
    let socket = candev::Socket::new("vcan0").expect("Could not open socketcan interface");
    socket.set_nonblocking(true).unwrap();
    let mut stack = j1939::stack::Stack::new(socket, StdTimer::new());

    let name = Name {
        address_capable: true,
        industry_group: IndustryGroup::OnHighway.into(),
        vehicle_system_instance: 0,
        vehicle_system: VehicleSystem1OnHighway::Trailer.into(),
        function: Functions::AxleDrive8.into(),
        function_instance: 2,
        ecu_instance: 0,
        manufacturer_coder: 0,
        identity_number: 0x12345,
    };

    let _cf = stack.register_control_function(0x80, name);

    loop {
        stack.process();
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
}
