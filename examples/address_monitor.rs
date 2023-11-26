use j1939::{
    self,
    frame::{Request, PGN_ADDRESSCLAIM},
};
use socketcan::{CanSocket, Socket};

fn main() {
    // create a socket and set to non blocking
    let socket = CanSocket::open("vcan0").unwrap();
    socket.set_nonblocking(true).unwrap();
    let mut stack = j1939::stack::Stack::new(socket, j1939::time::std::StdTimerDriver::new());

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
            println!("Request addresses");
        }
    }
}
