// use muexe_core::error;
// use muexe_core::machine::observer::{
//     ClonableObserver, ObservationKind, Observer, PCChange, PCChangeAction,
// };
// use muexe_core::pcode::{Instruction, PCode};
// use muexe_core::state::pcode::PCodeState;
// use muexe_core::state::AsState;
// use muexe_core::types::Address;
use crate::backend::InterruptError;
use std::marker::PhantomData;
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
use fuguex::concrete::hooks::{ClonableHookConcrete, HookConcrete};
use fuguex::hooks::types::{HookStepAction, HookOutcome, Error as HookError};

use crate::backend::InterruptHandlerOverrider;
use std::convert::TryInto;

// This case handles both "soft" and "hard" interrupts. Soft interrupts
// are those that we wish to handle by redirecting control to a new location
// in firmware. While hard interrupts are handled outside of the firmware,
// e.g., in Rust.
//

// Status of an interrupt
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Status {
    Disabled,
    Fired(usize),
    Pending,
}

impl Status {
    pub fn fire(self) -> Self {
        match self {
            Self::Disabled => Self::Disabled,
            Self::Fired(n) => Self::Fired(n + 1),
            Self::Pending => Self::Fired(1),
        }
    }

    pub fn unfire(self) -> Self {
        match self {
            Self::Disabled => Self::Disabled,
            Self::Pending => Self::Pending,
            Self::Fired(1) => Self::Pending,
            Self::Fired(n) => Self::Fired(n - 1),
        }
    }

    pub fn has_fired(&self) -> bool {
        matches!(self, Self::Fired(_))
    }
}

// An interrupt wrapper helps an interrupt handle its current state; essentially
// it lets the interrupt know if it has fired or not (by passing a Status to its
// status parameter), its nesting, if it is disabled, etc.
//
#[derive(Clone)]
pub struct InterruptWrapper<I, S, O>
where
    I: InterruptHandlerOverrider<State = S>,
    S: AsState<PCodeState<u8, O>>,
    O: Order
{
    interrupt: I,
    status: Status,
    handler_stack: Vec<Address>,
    returns_stack: Vec<Address>,
    allow_nesting: bool,
    order: PhantomData<O>,
}

impl<I, S, O> InterruptWrapper<I, S, O>
where
    I: InterruptHandlerOverrider<State = S>,
    S: AsState<PCodeState<u8, O>>,
    O: Order
{
    #[inline]
    pub fn is_disabled(&self) -> bool {
        self.status == Status::Disabled
    }

    #[inline]
    pub fn disable(&mut self) {
        self.status = Status::Disabled
    }

    #[inline]
    pub fn is_enabled(&self) -> bool {
        self.status != Status::Disabled
    }

    #[inline]
    pub fn enable(&mut self) {
        if self.is_disabled() {
            self.status = if self.returns_stack.is_empty() {
                Status::Pending
            } else {
                Status::Fired(self.returns_stack.len())
            };
        }
    }

    #[inline]
    pub fn is_pending(&self) -> bool {
        self.status == Status::Pending
    }

    #[inline]
    pub fn has_fired(&self) -> bool {
        self.status.has_fired()
    }

    #[inline]
    pub fn can_fire(&self) -> bool {
        (self.status.has_fired() && self.allow_nesting) || self.is_pending()
    }

    pub fn new(interrupt: I, allow_nesting: bool) -> Self {
        Self {
            interrupt,
            status: Status::Pending,
            handler_stack: Vec::new(),
            returns_stack: Vec::new(),
            allow_nesting,
            order: PhantomData,
        }
    }

    pub fn interrupt(&self) -> &I {
        &self.interrupt
    }

    pub fn interrupt_mut(&mut self) -> &mut I {
        &mut self.interrupt
    }
}

impl<I: 'static, S: 'static, O> HookConcrete for InterruptWrapper<I, S, O>
where
    I: Clone + InterruptHandlerOverrider<State = S>,
    S: AsState<PCodeState<u8, O>> + StateOps, 
    O: Order
{
    type State = S;
    type Error = InterruptError;
    type Outcome = String;
    
    
    fn hook_architectural_step(&mut self, state: &mut Self::State, address: &Address, operation: &StepState)
        -> Result<HookOutcome<HookStepAction<Self::Outcome>>, HookError<Self::Error>> 
    {
        // The address is the target to branch to

        // We are back at the point we got fired from; pop
        if self.has_fired()
            && self
                .returns_stack
                .last()
                .map(|addr| *address == *addr)
                .unwrap_or(false)
        {
            self.returns_stack.pop();
            self.status = self.status.unfire();
        }

        // We are in the handler; do not allow any nesting on this address
        if self.has_fired()
            && self
                .handler_stack
                .last()
                .map(|addr| *address == *addr)
                .unwrap_or(false)
        {
            self.handler_stack.pop();
            return Ok(HookStepAction::Pass.into());
            // return Ok(PCChangeAction::Execute);
        }

        let status = if let Status::Fired(v) = self.status {
            v
        } else {
            0
        };

        match self
            .interrupt
            .fire(status.try_into().unwrap(), state, address, operation)?
        {
            None => Ok(HookStepAction::Pass.into()),
            Some(handler_address) => {
                self.handler_stack.push(handler_address);
                self.returns_stack.push(address.clone());
                self.status = self.status.fire();
                // Skip to handler
                let hook_outcome: HookOutcome<_> = HookStepAction::Branch((1, handler_address)).into();
                Ok(hook_outcome.state_changed(true))       // Indicate this hook has changed the next address to be executed
            }
        }
    }
}

impl<I: 'static, S: 'static, O> ClonableHookConcrete for InterruptWrapper<I, S, O>
where I: Clone + InterruptHandlerOverrider<State = S>,
      S: AsState<PCodeState<u8, O>> + StateOps,
      O: Order
{
}

// #[cfg(test)]
// mod test {
//     use super::*;
//     use muexe_core::types::{LE, Order, Word};
//     use std::marker::PhantomData;
//     use std::time::{Duration, Instant};

//     #[derive(Clone)]
//     pub struct TimerInterrupt<O: Order, W: Word> {
//         handler: Address,
//         period: Duration,
//         last_measured: Instant,
//         order: PhantomData<O>,
//         word: PhantomData<W>,
//     }

//     impl<O: Order, W: Word> TimerInterrupt<O, W> {
//         pub fn new(handler: Address, period: Duration) -> Self {
//             Self {
//                 handler,
//                 period,
//                 last_measured: Instant::now(),
//                 order: PhantomData,
//                 word: PhantomData,
//             }
//         }
//     }

//     impl<O: Order, W: Word> InterruptHandlerOverrider for TimerInterrupt<O, W> {
//         type State = PCodeState;

//         fn fire(&mut self, _status: u128, _state: &mut Self::State, _address: Address, _pcode: &[PCode], _instruction: &Instruction) -> Result<Option<Address>, error::Error> {
//             let next = Instant::now();
//             if next.duration_since(self.last_measured) >= self.period {
//                 self.last_measured = next;
//                 Ok(Some(self.handler))
//             } else {
//                 Ok(None)
//             }
//         }
//     }

//     #[test]
//     fn simple_setup() -> Result<(), error::Error> {
//         let _timer = InterruptWrapper::new(TimerInterrupt::<LE, u32>::new(Address::from(0xdead), Duration::from_millis(1000)), false);
//         Ok(())
//     }
// }

