use std::sync::Mutex;
use std::sync::Arc;
// use std::collections::{HashMap, HashSet};
use std::marker::PhantomData;
use thiserror::Error;

use crate::polling::{
    PollingPeripheralHandler,
    Error as PoolingHandlerError,
};

use fugue::ir::{
    Address,
};
use fuguex::state::{
    State,
    pcode::PCodeState,
    StateOps,
    pcode::Error as PCodeError
};
use fuguex::concrete::hooks::{ClonableHookConcrete, HookConcrete};
use fugue::bytes::{Order};
use fuguex::hooks::types::{HookAction, HookOutcome, Error as HookError};

#[derive(Debug, Error)]
pub enum MyError {
    #[error(transparent)]
    PCode(#[from] PCodeError),
    #[error("`{0}` is not a valid register for the specified architecture")]
    InvalidRegister(String),

    #[error("Mem pooling handle input failed")]
    MemoryPoolingHandleInputFailed {source: PoolingHandlerError},
    #[error("Mem pooling handle output failed")]
    MemoryPoolingHandleOutputFailed {source: PoolingHandlerError},
}

impl From<PoolingHandlerError> for HookError<PCodeError> {
    fn from(e: PoolingHandlerError) -> Self {
        HookError::Hook(PCodeError::UnsupportedAddressSize(0))
    }
}

// S: State which impl AsState<PCodeState<u8, Order>>
// P: PollingPeripheral<Input=Address, Output=Address, State=S>
// Order: Endian
#[derive(Debug, Clone)]
pub struct MemoryPollingPeripheral<S, P, O> 
    where P: PollingPeripheralHandler<Input=Address, Output=Address, Order=O>,
          S: StateOps,
          O: Order,
{
    address_range: (Address, Address),
    peripheral: Arc<Mutex<P>>, // TODO: maybe use Cell?
    state: PhantomData<S>,
}

impl<S, P, O> MemoryPollingPeripheral<S, P, O> 
    where P: PollingPeripheralHandler<Input=Address, Output=Address, Order=O>,
          S: StateOps,
          O: Order,
{
    pub fn peripheral(&self) -> Arc<Mutex<P>> {
        self.peripheral.clone()
        // &self.peripheral
    }

    pub fn peripheral_mut(&mut self) -> Arc<Mutex<P>> {
        self.peripheral.clone()
    }
}

pub struct MemoryPollingPeripheralBuilder<S, P, O> 
    where P: PollingPeripheralHandler<Input=Address, Output=Address, Order=O> {
    peripheral: P,
    state: PhantomData<S>,
    address_range: (Address, Address)
}

// Address_range: (start, end)
impl<S, P, O> MemoryPollingPeripheralBuilder<S, P, O> 
    where P: PollingPeripheralHandler<Input=Address, Output=Address, Order=O>,
          S: State + StateOps,
          O: Order,
{
    pub fn new(peripheral_in: P, muexe_state: &mut PCodeState<u8, O>, address_range: (Address, Address)) -> Result<Self, PCodeError> {
        let mut sel = Self {
            peripheral : peripheral_in,
            state: PhantomData,
            address_range
        };
        sel.peripheral.init(muexe_state).unwrap();
        Ok(sel)
    }

    pub fn peripheral(mut self, peripheral: P) -> Self {
        self.peripheral =peripheral;
        self
    }

    pub fn build(self) -> Result<MemoryPollingPeripheral<S, P, O>, PCodeError> {
        Ok(MemoryPollingPeripheral {
            address_range: self.address_range,
            // regisiters: self.registers,
            peripheral: Arc::new(Mutex::new(self.peripheral)),
            state: self.state,
        })
    }
}

impl<S: 'static, P: 'static, O> HookConcrete for MemoryPollingPeripheral<S, P, O>
where S: State + StateOps ,
      P: PollingPeripheralHandler<Input=Address, Output=Address, Order=O> , 
      O: Order 
{
    type State = PCodeState<u8, O>;        // TOOD: make it useful for universal endian
    type Error = PCodeError;
    type Outcome = String;
    fn hook_memory_read(&mut self, state: &mut Self::State, address: &Address, size: usize) -> Result<HookOutcome<HookAction<Self::Outcome>>, HookError<Self::Error>> {
        let (min, max) = self.address_range;
        if min<= *address && *address<= max {
            self.peripheral.lock().unwrap().handle_input(state, &address, size)?;
        }
        Ok(HookAction::Pass.into())
    }
 
    fn hook_memory_write(&mut self, state: &mut Self::State, address: &Address, size: usize, value: &[u8]) ->  Result<HookOutcome<HookAction<Self::Outcome>>, HookError<Self::Error>>{
        let (min, max) = self.address_range;
        if min<= *address && *address <= max {
            self.peripheral.lock().unwrap().handle_output(state, &address, value, size)?;
        }
        Ok(HookAction::Pass.into())
    }
}

impl<S: 'static, P: 'static, O> ClonableHookConcrete for MemoryPollingPeripheral<S, P, O>
where S: State + StateOps,
      P: PollingPeripheralHandler<Input=Address, Output=Address, Order= O>, O: Order { }
