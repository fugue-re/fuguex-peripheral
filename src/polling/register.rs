// use muexe_core::error;
// use muexe_core::machine::observer::{ClonableObserver, Observer, ObservationKind};
// use muexe_core::machine::observer::{RegisterPreRead, RegisterWrite};
// use muexe_core::pcode::{self, Register, Sleigh};
// use muexe_core::state::AsState;
// use muexe_core::state::pcode::PCodeState;

use std::collections::{HashMap, HashSet};
use std::marker::PhantomData;
use fugue::ir::il::pcode::{Register};
use fugue::ir::translator::Translator;
use fuguex::concrete::hooks::{ClonableHookConcrete, HookConcrete};
use fuguex::state::{
    AsState, 
    pcode::PCodeState,
    pcode::Error as PCodeError,
};
use fugue::bytes::{Order};
use thiserror::Error;

use crate::polling::PollingPeripheral;
use crate::polling::Error as PoolingHandlerError;

#[derive(Debug, Error)]
pub enum Error {
    #[error(transparent)]
    PCode(#[from] PCodeError),
    #[error("`{0}` is not a valid register for the specified architecture")]
    InvalidRegister(String),
    
    #[error("Reg pooling handle input failed")]
    RegisterPoolingHandleInputFailed {source: PoolingHandlerError},
    #[error("Reg pooling handle output failed")]
    RegisterPoolingHandleOutputFailed {source: PoolingHandlerError},
}

#[derive(Debug, Clone)]
pub struct RegisterPollingPeripheral<S, P, O> 
    where P: PollingPeripheral<Input=Register, Output=Register, Order=O, State=S>, 
          O: Order
{
    inputs: HashSet<Register>,
    outputs: HashSet<Register>,
    peripheral: P,
    state: PhantomData<S>,
}

impl<S, P, O> RegisterPollingPeripheral<S, P, O> 
    where P: PollingPeripheral<Input=Register, Output=Register, Order=O, State=S>,
          O: Order  
{
    pub fn peripheral(&self) -> &P {
        &self.peripheral
    }

    pub fn peripheral_mut(&mut self) -> &mut P {
        &mut self.peripheral
    }
}

pub struct RegisterPollingPeripheralBuilder<S, P, O> 
    where P: PollingPeripheral<Input=Register, Output=Register, Order=O, State=S>,
          O: Order,
{
    inputs: HashSet<Register>,
    outputs: HashSet<Register>,
    registers: HashMap<String, Register>,
    peripheral: P,
    state: PhantomData<S>,
}

impl<S, P, O> RegisterPollingPeripheralBuilder<S, P, O> 
    where P: PollingPeripheral<Input=Register, Output=Register, Order=O, State=S>,
          O: Order,
{
    pub fn new(peripheral: P, translator: &Translator) -> Result<Self, Error> {
        Ok(Self {
            inputs: HashSet::new(),
            outputs: HashSet::new(),
            registers: translator.registers().iter().map(
                |(&(offset, size), &name)| (name.to_lowercase(), Register{name, offset, size})
            ).collect(),
            peripheral,
            state: PhantomData,
        })
    }

    pub fn peripheral(mut self, peripheral: P) -> Self {
        self.peripheral = peripheral;
        self
    }

    pub fn input_register<R: AsRef<str>>(mut self, name: R) -> Result<Self, Error> {
        let name = name.as_ref();
        let norm_name = name.to_lowercase();
        self.inputs.insert(self.registers.get(&norm_name).ok_or_else(|| Error::InvalidRegister(name.to_owned()))?.clone());
        Ok(self)
    }

    pub fn output_register<R: AsRef<str>>(mut self, name: R) -> Result<Self, Error> {
        let name = name.as_ref();
        let norm_name = name.to_lowercase();
        self.outputs.insert(self.registers.get(&norm_name).ok_or_else(|| Error::InvalidRegister(name.to_owned()))?.clone());
        Ok(self)
    }

    pub fn build(self) -> Result<RegisterPollingPeripheral<S, P, O>, Error> {
        Ok(RegisterPollingPeripheral {
            inputs: self.inputs,
            outputs: self.outputs,
            peripheral: self.peripheral,
            state: self.state,
        })
    }
}

impl<S, P, O> HookConcrete for RegisterPollingPeripheral<S, P, O>
where S: AsState<PCodeState<u8, O>>,
      P: PollingPeripheral<Input=Register, Output=Register, Order=O, State=S> {
    type State = S;
    fn hook_register_read(&mut self, state: &mut Self::State, register: &Register) -> Result<(), Error> {
        if self.inputs.contains(register) {
            self.peripheral.handle_input(state, register).map_err( |e| Error::RegisterPoolingHandleInputFailed{source: e} )?;
        }
        Ok(())
    }
    fn hook_register_write(&mut self, state: &mut Self::State, register: &Register, value: &[u8]) -> Result<(), Error> {
        if self.outputs.contains(register) {
            self.peripheral.handle_output(state, register, value).map_err( |e| Error::RegisterPoolingHandleOutputFailed{source: e} )?;
        }
        Ok(())
    }
}

impl<S, P, O> ClonableHookConcrete for RegisterPollingPeripheral<S, P, O>
where S: AsState<PCodeState<u8, O>> + Clone,
      P: PollingPeripheral<Input=Register, Output=Register, Order=O, State=S> { }
