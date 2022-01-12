
use std::marker::PhantomData;
use thiserror::Error;
use fugue::ir::{
    Address,
};
use fuguex::state::{
	AsState,
    StateOps,
    pcode::PCodeState,
};
use fuguex::machine::StepState;
use fugue::bytes::{Order};
use fuguex::hooks::types::Error as HookError;

#[derive(Debug, Error)]
pub enum InterruptError {
    #[error("Interrupt Error")]
    DefaultError(),
}


impl From<InterruptError> for HookError<InterruptError> {
    fn from(error: InterruptError) -> HookError<InterruptError> {
        HookError::Hook(error)
    }
}

// If you need to override a interrupt handler then impl this trait
pub trait InterruptHandlerOverrider: {
    type State: AsState<PCodeState<u8, Self::Endian>>;
    type Endian: Order;
    fn fire(
        &mut self,
        trigger_count: u128,
        state: &mut Self::State,
        address: &Address,
        operation: &StepState,
     ) -> Result<Option<Address>, InterruptError>;
}

#[derive(Clone)]
pub struct EmptyInterruptHandlerOverrider<S: AsState<PCodeState<u8,E>>, E: Order > { state: PhantomData<S>, endian: PhantomData<E> }
impl <S: AsState<PCodeState<u8, E>>, E: Order>InterruptHandlerOverrider for EmptyInterruptHandlerOverrider<S, E> {
    
    type State = S;
    type Endian = E;

    fn fire(
        &mut self,
        _trigger_count: u128,
        _state: &mut Self::State,
        _address: &Address,
        _operation: &StepState,
    ) -> Result<Option<Address>, InterruptError> {
        Ok(None)
    }

}

// Interrupt Handler Types
#[derive(Clone)]
pub enum InterruptHandler <O: InterruptHandlerOverrider> {
    Routine(Address),       // Address of a routine
    Vector(Address),        // Address of a pointer to a routine
    Override(O),            // Override function written in Rust
}
// TODO: Add a function to be called in observer PC Change
impl <O: InterruptHandlerOverrider>InterruptHandler<O> {
    pub fn get_routine_address<S: AsState<PCodeState<u8, E>>, E: Order>(&self, state: &S) -> Option<Address>{
        match self {
            Self::Routine(a) => {
                // Return the address
                Some(a.clone())
            },
            Self::Vector(a) => {
                // Obtain the real address from the pointer variable
                let target_addr : u32= E::read_u32(state.state_ref().view_values(*a, 4).unwrap());
                Some(Address::from(target_addr))
            },
            Self::Override(_o) => {
                None
            }
        }

    }
}

#[derive(Clone)]
pub struct Interrupt  {
    name: String,
	enabled: bool,
    triggered: bool, 
    trigger_count: u128,
    priority: i32,
}

impl Interrupt {
    pub fn new(name: &str) -> Self{
        Self {
            name: String::from(name),
            enabled: false,
            triggered: false,
            trigger_count: 0,
            priority: 0,
        }
    }

    pub fn set_triggered(&mut self, val: bool) {
        self.triggered = val;
    }

    pub fn is_triggered(&self) -> bool {
        self.triggered
    }

    pub fn set_enable(&mut self, val: bool) {
        self.enabled = val;
    }

    pub fn is_enabled(&self) -> bool{
        self.enabled
    }

    pub fn set_priority(&mut self, val: i32){
        self.priority = val;
    }

    pub fn get_priority(&self) -> i32 {
        self.priority
    }

    pub fn add_trigger_count(&mut self) {
        self.trigger_count += 1;
    }

    pub fn get_trigger_count(&self) -> u128{
        self.trigger_count
    }

    pub fn get_name(&self) -> &str{
        &self.name
    }

}
