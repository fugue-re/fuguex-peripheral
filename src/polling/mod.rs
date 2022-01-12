use fugue::bytes::{Order};
use thiserror::Error;
use thiserror;

use fuguex::state::{
    pcode::PCodeState,
};

pub mod memory;
pub use memory::{MemoryPollingPeripheral, MemoryPollingPeripheralBuilder};
#[derive(Debug, Error)]
pub enum Error {
    // #[error(transparent)]
    // OtherError(#[from] PCodeError),
    #[error("`{0}` is not a valid register for the specified architecture")]
    InvalidRegister(String),
    #[error("handle input failed")]
    HandleInputFailed,
    #[error("handle output failed")]
    HandleOutputFailed,
    #[error("Init failed")]
    InitFailed,
    #[error("OtherError: faile at component {0}")]
    HandlerError(String),
    
}



pub trait PollingPeripheralHandler: Clone {
    type Input;
    type Output;
    type Order : Order;

    fn init(&mut self, state: &mut  PCodeState<u8, Self::Order>) -> Result<(), Error>;
    fn handle_input(&mut self, state: &mut  PCodeState<u8, Self::Order>, input: &Self::Input, size: usize) -> Result<(), Error>;
    fn handle_output(&mut self, state: &mut  PCodeState<u8, Self::Order>, output: &Self::Output, value: &[u8], size: usize) -> Result<(), Error>;
}
