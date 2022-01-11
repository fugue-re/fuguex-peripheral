use std::sync::Mutex;
use std::sync::Arc;
// use std::collections::{HashMap, HashSet};
use std::marker::PhantomData;
use thiserror::Error;

use crate::polling::{
    PollingPeripheral,
    Error as PoolingHandlerError,
};

use fugue::ir::{
    Address,
};
use fuguex::state::{
    AsState, 
    pcode::PCodeState,
    pcode::Error as PCodeError
    
};
use fuguex::concrete::hooks::{ClonableHookConcrete, HookConcrete};
use fugue::bytes::{Order};


#[derive(Debug, Error)]
pub enum Error {
    #[error(transparent)]
    PCode(#[from] PCodeError),
    #[error("`{0}` is not a valid register for the specified architecture")]
    InvalidRegister(String),

    #[error("Mem pooling handle input failed")]
    MemoryPoolingHandleInputFailed {source: PoolingHandlerError},
    #[error("Mem pooling handle output failed")]
    MemoryPoolingHandleOutputFailed {source: PoolingHandlerError},
}

// S: State which impl AsState<PCodeState<u8, Order>>
// P: PollingPeripheral<Input=Address, Output=Address, State=S>
// Order: Endian
#[derive(Debug, Clone)]
pub struct MemoryPollingPeripheral<S, P, O> 
    where P: PollingPeripheral<Input=Address, Output=Address, Order=O, State=S>,
          O: Order,
{
    address_range: (Address, Address),
    peripheral: Arc<Mutex<P>>, // TODO: maybe use Cell?
    state: PhantomData<S>,
}

impl<S, P, O> MemoryPollingPeripheral<S, P, O> 
    where P: PollingPeripheral<Input=Address, Output=Address, Order=O, State=S>,
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
    where P: PollingPeripheral<Input=Address, Output=Address, Order=O, State=S> {
    peripheral: P,
    state: PhantomData<S>,
    address_range: (Address, Address)
}

// Address_range: (start, end)
impl<S, P, O> MemoryPollingPeripheralBuilder<S, P, O> 
    where P: PollingPeripheral<Input=Address, Output=Address, Order=O, State=S>,
            O: Order,
{
    pub fn new(peripheral_in: P, muexe_state: &mut S, address_range: (Address, Address)) -> Result<Self, Error> {
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

    pub fn build(self) -> Result<MemoryPollingPeripheral<S, P, O>, Error> {
        Ok(MemoryPollingPeripheral {
            address_range: self.address_range,
            // regisiters: self.registers,
            peripheral: Arc::new(Mutex::new(self.peripheral)),
            state: self.state,
        })
    }
}

impl<S, P, O> HookConcrete for MemoryPollingPeripheral<S, P, O>
where S: AsState<PCodeState<u8, O>>,
      P: PollingPeripheral<Input=Address, Output=Address, Order=O, State=S> {
    type State = S;
    fn hook_memory_read(&mut self, state: &mut Self::State, address: &Address, size: usize) -> Result<(), Error> {
        let (min, max) = self.address_range;
        if min<= *address && *address<= max {
            self.peripheral.lock().unwrap().handle_input(state, &address).map_err(|e| Error::MemoryPoolingHandleInputFailed{source: e})?;
        }
        Ok(())
    }

    fn hook_memory_write(&mut self, state: &mut Self::State, address: &Address, size: usize, value: &[u8]) -> Result<(), Error> {
        let (min, max) = self.address_range;
        if min<= *address && *address <= max {
            self.peripheral.lock().unwrap().handle_output(state, &address, value).map_err(|e| Error::MemoryPoolingHandleOutputFailed{source: e})?;
        }
        Ok(())
    }
}

impl<S, P, O> ClonableHookConcrete for MemoryPollingPeripheral<S, P, O>
where S: AsState<PCodeState<u8, O>> + Clone,
      P: PollingPeripheral<Input=Address, Output=Address, Order= O, State=S> { }
