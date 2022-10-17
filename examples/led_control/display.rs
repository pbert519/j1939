//! control panel / display simulation
//! demonstrates the ecu features

use crate::{messages::*, StdTimer};
use j1939::{
    frame::*,
    name::*,
    stack::{ControlFunctionHandle, Stack},
};
use std::time::{Duration, Instant};

pub struct Display {
    cf_handle: ControlFunctionHandle,
    status_send_timestamp: Instant,
    step: Step,
}

enum Step {
    ConfigColor,
    ConfigCycle,
    ConfigDenied,
    ConfigRead,
    CommandOn,
    CommandOff,
    Request,
}

impl Display {
    pub fn new(stack: &mut Stack<impl embedded_can::blocking::Can, StdTimer>) -> Self {
        let cf_handle = stack.register_control_function(
            0x81,
            Name {
                address_capable: true,
                industry_group: IndustryGroup::AgriculturalAndForestry.into(),
                vehicle_system_instance: 0,
                vehicle_system: VehicleSystem2Agriculture::Tractor.into(),
                function: Functions::CabDisplay60.into(),
                function_instance: 0,
                ecu_instance: 1,
                manufacturer_coder: ManufacturerCodes::Reserved0.into(),
                identity_number: 0x123456,
            },
        );
        Self {
            cf_handle,
            status_send_timestamp: Instant::now(),
            step: Step::ConfigColor,
        }
    }

    pub fn process(&mut self, stack: &mut Stack<impl embedded_can::blocking::Can, StdTimer>) {
        let cf = stack.control_function(&self.cf_handle);
        // wait till ecu finished address claiming
        if cf.is_online().is_none() {
            return;
        }

        // ToDo handle source and ignore other messages
        while let Some(msg) = cf.get_frame() {
            match msg.header().pgn() {
                led_status::PGN_LED_STATUS => {
                    let status: led_status::LedStatus = msg.into();
                    println!("Display got status: {:?}", status)
                }
                PGN_ACK => {
                    let ack: Ack = msg.try_into().unwrap();
                    println!("Display got Ack: {:?}", ack)
                }
                led_control::PGN_LED_CONTROL => {
                    let led_control: led_control::LedControl = msg.into();
                    println!("Display got LedControl: {:?}", led_control)
                }
                _ => (),
            }
        }

        if Instant::now() > self.status_send_timestamp + Duration::from_millis(2000) {
            self.status_send_timestamp = Instant::now();

            let output_frame = match self.step {
                Step::ConfigColor => {
                    self.step = Step::ConfigCycle;
                    led_control::LedControl {
                        command: led_control::Command::Write,
                        parameter_id: led_control::Parameter::Color,
                        parameter_value: 0x00AABBCC,
                    }
                    .to_frame(0x80)
                }
                Step::ConfigCycle => {
                    self.step = Step::ConfigDenied;
                    led_control::LedControl {
                        command: led_control::Command::Write,
                        parameter_id: led_control::Parameter::CycleTimeMs,
                        parameter_value: 750,
                    }
                    .to_frame(0x80)
                }
                Step::ConfigDenied => {
                    self.step = Step::ConfigRead;
                    led_control::LedControl {
                        command: led_control::Command::Write,
                        parameter_id: led_control::Parameter::LastChangeMs,
                        parameter_value: 750,
                    }
                    .to_frame(0x80)
                }
                Step::ConfigRead => {
                    self.step = Step::CommandOn;
                    led_control::LedControl {
                        command: led_control::Command::Read,
                        parameter_id: led_control::Parameter::LastChangeMs,
                        parameter_value: 0xFFFFFFFF,
                    }
                    .to_frame(0x80)
                }
                Step::CommandOn => {
                    self.step = Step::CommandOff;
                    led_command::LedCommand { on: true }.to_frame(0x80)
                }
                Step::CommandOff => {
                    self.step = Step::Request;
                    led_command::LedCommand { on: false }.to_frame(0x80)
                }
                Step::Request => {
                    self.step = Step::ConfigColor;
                    Request::new(led_status::PGN_LED_STATUS, 0xFF, 0x80).into()
                }
            };
            cf.send_frame(output_frame);
        }
    }
}
