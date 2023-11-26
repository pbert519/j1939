//! J1939 ECU
//! The ecu implements a controller for a rgb led
//! The led can have different colors and blinks in various frequencies
//! As this example should run everywhere the controller does not actually control a rgb led but only implements the j1939 control interface
//! Provides two different messages / pgns
//! 1) PGN 0x00FF01 - Cyclic broadcast 1000ms - led status
//! 2) PGN 0x00EF00 - Peer to peer with group function values - PGN 0x0000 - read and write led parameter
//! Additional a command message PGN 0x002000 is received by the ecu to enable the led
//! The messages are defined in the messages.rs file
use crate::messages::*;
use j1939::{
    frame::*,
    name::*,
    stack::{ControlFunctionHandle, Stack},
    time::std::StdTimerDriver,
};
use std::time::{Duration, Instant};

pub struct ECU {
    led_on: bool,
    led_color: u32,
    led_cycle_time_ms: u32,
    cf_handle: ControlFunctionHandle,
    status_send_timestamp: Instant,
    last_modification_timestamp: Instant,
}

impl ECU {
    pub fn new(stack: &mut Stack<impl embedded_can::blocking::Can, StdTimerDriver>) -> Self {
        let ecu_name = Name {
            address_capable: true,
            industry_group: IndustryGroup::AgriculturalAndForestry.into(),
            vehicle_system_instance: 0,
            vehicle_system: VehicleSystem2Agriculture::Tractor.into(),
            function: Functions::BodyController26.into(),
            function_instance: 0,
            ecu_instance: 0,
            manufacturer_coder: ManufacturerCodes::Reserved0.into(),
            identity_number: 0x123456,
        };

        let cf_handle = stack.register_control_function(0x80, ecu_name);

        Self {
            led_on: false,
            led_color: 0xFFFFFF,
            led_cycle_time_ms: 500,
            cf_handle,
            status_send_timestamp: Instant::now(),
            last_modification_timestamp: Instant::now(),
        }
    }

    pub fn process(&mut self, stack: &mut Stack<impl embedded_can::blocking::Can, StdTimerDriver>) {
        let cf = stack.control_function(&self.cf_handle);
        // wait till ecu finished address claiming
        if cf.is_online().is_none() {
            return;
        }

        while let Some(msg) = cf.get_frame() {
            match msg.header().pgn() {
                PGN_REQUEST => {
                    cf.send_frame(self.handle_request(msg));
                }
                led_control::PGN_LED_CONTROL => {
                    cf.send_frame(self.handle_led_control(msg));
                }
                led_command::PGN_LED_COMMAND => {
                    cf.send_frame(self.handle_led_command(msg));
                }
                _ => (),
            }
        }

        if Instant::now() > self.status_send_timestamp + Duration::from_millis(1000) {
            self.status_send_timestamp = Instant::now();
            // send status message broadcast
            let status = led_status::LedStatus {
                on: self.led_on,
                color: self.led_color,
                cycle_time_ms: self.led_cycle_time_ms,
            };
            cf.send_frame(status.into());
        }
    }

    // on receive of a request message:
    // if the led_status is requested, response with a led status message
    // for all other pgn respond with a negative ack
    fn handle_request(&mut self, msg: Frame) -> Frame {
        let req: Request = msg.try_into().unwrap();
        println!(
            "ECU got a request for PGN: {:?} from address: {}",
            req.pgn(),
            req.header().source_address()
        );
        match *req.pgn() {
            led_status::PGN_LED_STATUS => led_status::LedStatus {
                on: self.led_on,
                color: self.led_color,
                cycle_time_ms: self.led_cycle_time_ms,
            }
            .into(),
            _ => Ack::new(
                AckType::NegativeAck,
                None,
                *req.pgn(),
                0xFF, /* my address is inserted by cf */
                req.header().source_address(),
            )
            .into(),
        }
    }

    fn handle_led_control(&mut self, msg: Frame) -> Frame {
        let sa = msg.header().source_address();
        let msg: led_control::LedControl = msg.into();
        println!("ECU got a led control message: {:?}", msg);

        match msg.command {
            led_control::Command::Read => self.read_command(msg.parameter_id, sa),
            led_control::Command::Write => {
                self.write_command(msg.parameter_id, msg.parameter_value, sa)
            }
            // respond with nack for unknown commands
            _ => Ack::new(
                AckType::NegativeAck,
                None,
                led_control::PGN_LED_CONTROL,
                0xFF, /* my address is inserted by cf */
                sa,
            )
            .into(),
        }
    }

    fn read_command(&mut self, parameter_id: led_control::Parameter, sa: u8) -> Frame {
        if let Some(parameter_value) = match parameter_id {
            led_control::Parameter::Color => Some(self.led_color),
            led_control::Parameter::CycleTimeMs => Some(self.led_cycle_time_ms),
            led_control::Parameter::LastChangeMs => {
                Some(self.last_modification_timestamp.elapsed().as_millis() as u32)
            }
            led_control::Parameter::Other(_) => None,
        } {
            led_control::LedControl {
                command: led_control::Command::ReadResponse,
                parameter_id,
                parameter_value,
            }
            .to_frame(sa)
        } else {
            Ack::new(
                AckType::NegativeAck,
                Some(led_control::Command::Read.into()),
                led_control::PGN_LED_CONTROL,
                0xFF, /* my address is inserted by cf */
                sa,
            )
            .into()
        }
    }

    fn write_command(
        &mut self,
        parameter_id: led_control::Parameter,
        parameter_value: u32,
        sa: u8,
    ) -> Frame {
        let ack_type = match parameter_id {
            led_control::Parameter::Color => {
                self.led_color = parameter_value;
                self.last_modification_timestamp = Instant::now();
                AckType::PositiveAck
            }
            led_control::Parameter::CycleTimeMs => {
                self.led_cycle_time_ms = parameter_value;
                self.last_modification_timestamp = Instant::now();
                AckType::PositiveAck
            }
            led_control::Parameter::LastChangeMs => AckType::AccessDenied,
            led_control::Parameter::Other(_) => AckType::NegativeAck,
        };

        Ack::new(
            ack_type,
            Some(led_control::Command::Write.into()),
            led_control::PGN_LED_CONTROL,
            0xFF, /* my address is inserted by cf */
            sa,
        )
        .into()
    }

    // on receive of a led control message switch led and send ack
    fn handle_led_command(&mut self, msg: Frame) -> Frame {
        let sa = msg.header().source_address();
        let msg: led_command::LedCommand = msg.into();
        self.led_on = msg.on;
        println!("ECU got a led command message: {:?}", msg);

        Ack::new(
            AckType::PositiveAck,
            None,
            led_command::PGN_LED_COMMAND,
            0xFF, /* overwritten */
            sa,
        )
        .into()
    }
}
