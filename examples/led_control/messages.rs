//! proprietary frame with group function values to read and write different parameter
//! all parameters have a u32 bit value without any factor or offset for simplicity
//! the message has the peer to peer pgn 0xEF00

use j1939::frame::*;

pub mod led_command {
    use super::*;

    pub const PGN_LED_COMMAND: PGN = PGN(0x002000);
    pub const PRIO_LED_COMMAND: u8 = 6;

    #[derive(Debug, PartialEq, Eq, Clone)]
    pub struct LedCommand {
        pub on: bool,
    }
    impl From<Frame> for LedCommand {
        fn from(frame: Frame) -> Self {
            Self {
                on: frame.data()[0] > 0,
            }
        }
    }
    impl LedCommand {
        pub fn to_frame(self, destination_address: u8) -> Frame {
            let header = Header::new(
                PGN_LED_COMMAND,
                PRIO_LED_COMMAND,
                0x00,
                Some(destination_address),
            );
            let mut data = [0xFF; 8];
            data[0] = self.on as u8;

            Frame::new(header, &data)
        }
    }
}

pub mod led_status {
    use super::*;

    pub const PGN_LED_STATUS: PGN = PGN(0x00FF01);
    pub const PRIO_LED_STATUS: u8 = 6;

    #[derive(Debug, PartialEq, Eq, Clone)]
    pub struct LedStatus {
        pub on: bool,
        pub color: u32,
        pub cycle_time_ms: u32,
    }
    impl From<Frame> for LedStatus {
        fn from(frame: Frame) -> Self {
            Self {
                on: frame.data()[0] > 0,
                color: u32::from_be_bytes(frame.data()[0..4].try_into().unwrap()) & 0x00FFFFFF,
                cycle_time_ms: u32::from_be_bytes(frame.data()[4..8].try_into().unwrap()),
            }
        }
    }
    impl From<LedStatus> for Frame {
        fn from(led_status: LedStatus) -> Self {
            let header = Header::new(PGN_LED_STATUS, PRIO_LED_STATUS, 0x00, None);
            let mut data = [0xFF; 8];
            data[0] = led_status.on as u8;
            // color
            data[1] = (led_status.color >> 16) as u8;
            data[2] = (led_status.color >> 8) as u8;
            data[3] = led_status.color as u8;
            // cycle time
            data[4] = (led_status.cycle_time_ms >> 24) as u8;
            data[5] = (led_status.cycle_time_ms >> 16) as u8;
            data[6] = (led_status.cycle_time_ms >> 8) as u8;
            data[7] = led_status.cycle_time_ms as u8;

            Frame::new(header, &data)
        }
    }
}

pub mod led_control {
    use super::*;

    pub const PGN_LED_CONTROL: PGN = PGN(0x00EF00);
    pub const PRIO_LED_CONTROL: u8 = 6;

    #[derive(Debug, PartialEq, Eq, Clone)]
    pub struct LedControl {
        pub command: Command,
        pub parameter_id: Parameter,
        pub parameter_value: u32,
    }
    impl From<Frame> for LedControl {
        fn from(frame: Frame) -> Self {
            let parameter_value: u32 =
                u32::from_be_bytes(frame.data()[3..7].try_into().unwrap()).into();

            Self {
                command: frame.data()[0].into(),
                parameter_id: u16::from_be_bytes(frame.data()[1..3].try_into().unwrap()).into(),
                parameter_value,
            }
        }
    }
    impl LedControl {
        pub fn to_frame(self, destination_address: u8) -> Frame {
            let header = Header::new(
                PGN_LED_CONTROL,
                PRIO_LED_CONTROL,
                0x00,
                Some(destination_address),
            );
            let mut data = [0xFF; 8];
            data[0] = self.command.into();
            let parameter_id: u16 = self.parameter_id.into();
            data[1] = (parameter_id >> 8) as u8;
            data[2] = parameter_id as u8;
            data[3] = (self.parameter_value >> 24) as u8;
            data[4] = (self.parameter_value >> 16) as u8;
            data[5] = (self.parameter_value >> 8) as u8;
            data[6] = self.parameter_value as u8;

            Frame::new(header, &data)
        }
    }
    #[derive(Debug, PartialEq, Eq, Clone)]
    pub enum Command {
        Read,
        Write,
        ReadResponse,
        Other(u8),
    }
    impl From<u8> for Command {
        fn from(raw: u8) -> Self {
            match raw {
                0 => Command::Read,
                1 => Command::ReadResponse,
                2 => Command::Write,
                _ => Command::Other(raw),
            }
        }
    }
    impl From<Command> for u8 {
        fn from(cmd: Command) -> Self {
            match cmd {
                Command::Read => 0,
                Command::ReadResponse => 1,
                Command::Write => 2,
                Command::Other(raw) => raw,
            }
        }
    }

    #[derive(Debug, PartialEq, Eq, Clone)]
    pub enum Parameter {
        Color,
        CycleTimeMs,
        LastChangeMs,
        Other(u16),
    }
    impl From<u16> for Parameter {
        fn from(raw: u16) -> Self {
            match raw {
                0 => Parameter::Color,
                1 => Parameter::CycleTimeMs,
                2 => Parameter::LastChangeMs,
                _ => Parameter::Other(raw),
            }
        }
    }
    impl From<Parameter> for u16 {
        fn from(param: Parameter) -> Self {
            match param {
                Parameter::Color => 0,
                Parameter::CycleTimeMs => 1,
                Parameter::LastChangeMs => 2,
                Parameter::Other(raw) => raw,
            }
        }
    }
    mod tests {
        use super::*;

        #[test]
        fn test_message_coding() {
            let prop_msg = LedControl {
                command: Command::ReadResponse,
                parameter_id: Parameter::Other(0xF1CA),
                parameter_value: 0x12345678,
            };
            let frame = prop_msg.clone().to_frame(0x30);
            assert_eq!(prop_msg, frame.into())
        }
    }
}
