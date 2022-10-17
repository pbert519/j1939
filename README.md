# A J1939 Stack written Rust

The stack is still work in progress!

## Features:
- no_std support, but requires alloc
- supports all drivers based on the embedded_can::blocking trait 
- Address management (except NAME command and Address command)
- P2P and broadcast transport protocols
- NEMA2000 fast packet transport protocol

## Examples
All examples are configured to use a socketcan interface named 'vcan0'.
On linux you can create a virtual socketcan interface.
On other platforms it is necessary to use a different driver which implements the embedded_can driver traits.

### Address Monitor
This example list all control functions on a j1939 bus with the currently used address and name.
To keep the list always updated, the examples send a request for address claim every second, on which all active control functions have to react.

### EEC1 Receive
The example listens on the bus for the electronic engine controller broadcast message and prints the raw contents on the command line.

### LED Control
The example consists of two J1939 control functions, which demonstrate the communication between a "ecu" driving a rgb led and a "display" to control the led.
Both control functions participate in address management and claiming their address.
The examples uses multiple proprietary messages:
- **led_status**:       broadcast the current status of the led every 1000ms on the bus
- **led_command**:      controls the on/off state of the led
- **led_control**:      read/write led parameters like cycle time and color

Additionally **Acknowledgement** and **Request** are used to respond to messages or request messages.


## ToDo

### General
- Logging support
- Better error handling
- Timings and Timeouts may be not standard conform or missing

### Examples
- other OS than linux / baremetal 

### Control Function
- PGN Filter

### Address Management
- Address command
- NAME change command
- SA violation check and handling

### Transport Protocols
- extended transport protocol support

