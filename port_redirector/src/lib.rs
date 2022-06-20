//! This is the PortRedirectorTool Crate. This tool is used for connecting single input ports (UDP, TCP or Serial) and retransmitting the incoming data 
//! to multiple sockets through a TCP server. This tool does not support sending data to the original port, only reading. It's used to redirecting 
//! data from a sensor to multiple endpoints.


pub mod input_stream;
pub mod retransmit_server;