use thiserror::Error;

use fuguex::state::{
    AsState, 
    pcode::PCodeState,
    State as FuguexState
};

pub mod register;
pub use register::{RegisterPollingPeripheral, RegisterPollingPeripheralBuilder};

pub mod memory;
pub use memory::{MemoryPollingPeripheral, MemoryPollingPeripheralBuilder};
#[derive(Debug, Error)]
pub enum Error {
    // #[error(transparent)]
    // OtherError(#[from] PCodeError),
    #[error("`{0}` is not a valid register for the specified architecture")]
    InvalidRegister(String),

    #[error("write to memory failed")]
    MemoryWriteFailed,
    #[error("read from memory failed")]
    MemoryReadFailed,
}



pub trait PollingPeripheral: Clone + Send + Sync {
    type Input;
    type Output;
    type Order;
    type State: AsState<PCodeState<u8, Self::Order>>;

    fn init(&mut self, state: &mut Self::State) -> Result<(), Error>;
    fn handle_input(&mut self, state: &mut Self::State, input: &Self::Input) -> Result<(), Error>;
    fn handle_output(&mut self, state: &mut Self::State, output: &Self::Output, value: &[u8]) -> Result<(), Error>;
}
