use std::sync::Arc;
use std::marker::PhantomData;
use thiserror::Error;
use byteorder::ByteOrder;
use crate::backend::EmptyInterruptHandlerOverrider;
use fugue::ir::{
    Address,
	Translator,
};
use fuguex::state::{
    AsState, 
    pcode::PCodeState,
    StateOps,
    pcode::Error as PCodeError
};
use fuguex::concrete::hooks::{ClonableHookConcrete, HookConcrete};
use fugue::bytes::{BE};
use fuguex::hooks::types::{HookStepAction, HookAction, HookOutcome, Error as HookError};
use fuguex::machine::StepState;

use crate::backend;
use crate::backend::compare_match_timer::FunName as CMTFunName;
use std::convert::TryInto;
use log::{info};

#[derive(Debug, Error)]
pub enum SuperHCMTError {
    #[error(transparent)]
    PCode(#[from] PCodeError),
    #[error("`{0}` is not a valid register for the specified architecture")]
    InvalidRegister(String),
}
impl From<SuperHCMTError> for HookError<SuperHCMTError> {
    fn from(error: SuperHCMTError) -> HookError<SuperHCMTError> {
        HookError::Hook(error)
    }
}

type Endian = BE;

#[derive(Debug)]
pub struct CompareMatchTimer <S, E>
where
	S: AsState<PCodeState<u8, Endian>>,
	E:  Send + Sync + 'static
{
	// translator: Arc<Translator>,
	backend: (backend::CompareMatchTimer, backend::CompareMatchTimer),
	interrupt: (backend::Interrupt, backend::Interrupt),
	handler: (backend::InterruptHandler<EmptyInterruptHandlerOverrider<S, Endian>>,
		backend::InterruptHandler<EmptyInterruptHandlerOverrider<S, Endian>>),
	address_range: (Address, Address),
	endian: PhantomData<Endian>,
	error: PhantomData<E>,
	
}
// NOTE: manual implementation avoids adding the trait bound `E: Clone`.
impl<S, E> Clone for CompareMatchTimer<S, E> 
where
	S: AsState<PCodeState<u8, Endian>>,
	E:  Send + Sync
{
    fn clone(&self) -> Self {
        Self {
			backend: self.backend.clone(),
			interrupt: self.interrupt.clone(),
			handler: self.handler.clone(),
			address_range: self.address_range.clone(),
			endian: self.endian.clone(),
			error: self.error.clone(),
        }
    }
}

impl<S: AsState<PCodeState<u8, Endian>>, E> CompareMatchTimer<S, E> 
where E:  Send + Sync
{
	pub fn new() -> Self{
		let mut cmt = backend::CompareMatchTimer::default();
		let mut cmt1 = backend::CompareMatchTimer::default();
		const ADDR_CMSTR:u64 = 0xfffec000; 
		const ADDR_CMCSR_0:u64 = 0xfffec002; 
		const ADDR_CMCNT_0:u64 = 0xfffec004;
		const ADDR_CMCOR_0:u64 = 0xfffec006;

		const ADDR_CMCSR_1:u64 = 0xfffec008;
		const ADDR_CMCNT_1:u64 = 0xfffec00a;
		const ADDR_CMCOR_1:u64 = 0xfffec00c;

		cmt.map_function_addr_read(ADDR_CMSTR, 0x01, 		&CMTFunName::is_enabled);
		cmt.map_function_addr_write(ADDR_CMSTR, 0x01, 		&CMTFunName::set_enable);

		cmt.map_function_addr_read(ADDR_CMCSR_0, 0x40, 		&CMTFunName::is_interrupt_enabled);
		cmt.map_function_addr_write(ADDR_CMCSR_0, 0x40, 	&CMTFunName::set_interrupt_enabled);

		cmt.map_function_addr_read(ADDR_CMCSR_0, 0x80, 		&CMTFunName::is_matched);
		cmt.map_function_addr_write(ADDR_CMCSR_0, 0x80, 	&CMTFunName::clear_matched_flag);

		cmt.map_function_addr_read(ADDR_CMCNT_0, 0xffff, 	&CMTFunName::get_current_tick);
		cmt.map_function_addr_write(ADDR_CMCNT_0, 0xffff, 	&CMTFunName::set_current_tick);

		cmt.map_function_addr_read(ADDR_CMCOR_0, 0xffff, 	&CMTFunName::get_compare_against);
		cmt.map_function_addr_write(ADDR_CMCOR_0, 0xffff, 	&CMTFunName::set_compare_against);
	

		cmt1.map_function_addr_read(ADDR_CMSTR, 0x02, 		&CMTFunName::is_enabled);
		cmt1.map_function_addr_write(ADDR_CMSTR, 0x02, 		&CMTFunName::set_enable);

		cmt1.map_function_addr_read(ADDR_CMCSR_1, 0x40, 		&CMTFunName::is_interrupt_enabled);
		cmt1.map_function_addr_write(ADDR_CMCSR_1, 0x40, 	&CMTFunName::set_interrupt_enabled);

		cmt1.map_function_addr_read(ADDR_CMCSR_1, 0x80, 		&CMTFunName::is_matched);
		cmt1.map_function_addr_write(ADDR_CMCSR_1, 0x80, 	&CMTFunName::clear_matched_flag);

		cmt1.map_function_addr_read(ADDR_CMCNT_1, 0xffff, 	&CMTFunName::get_current_tick);
		cmt1.map_function_addr_write(ADDR_CMCNT_1, 0xffff, 	&CMTFunName::set_current_tick);

		cmt1.map_function_addr_read(ADDR_CMCOR_1, 0xffff, 	&CMTFunName::get_compare_against);
		cmt1.map_function_addr_write(ADDR_CMCOR_1, 0xffff, 	&CMTFunName::set_compare_against);
		Self {
			// translator, 
			backend: (cmt, cmt1),
			interrupt: (backend::Interrupt::new("CMT0"), backend::Interrupt::new("CMT1")),
			handler : (backend::InterruptHandler::Vector(Address::from(0x000002BCu32)),   // CMI0 Channel Vector
						backend::InterruptHandler::Vector(Address::from(0x000002c0u32))), //CMI1 Channel Vector: 000002C0 IPR10 7-4, 3-0
			address_range: (Address::from(0xFFFEC000u32), Address::from(0xFFFEC00Cu32)), //Address::from(0xFFFEC00Cu32)),  
			endian: PhantomData,
			error: PhantomData,
		}
	}
}

impl <S: 'static, E> HookConcrete for CompareMatchTimer<S, E>
where
	S: AsState<PCodeState<u8, Endian>>,
	E: std::error::Error +  Send + Sync + 'static
{

	type State = PCodeState<u8, Endian>;
	type Error = E;
	type Outcome = String;

	fn hook_architectural_step(&mut self, state: &mut Self::State, address: &Address, _operation: &StepState)
		-> Result<HookOutcome<HookStepAction<Self::Outcome>>, HookError<Self::Error>>  {

		// println!("sp: {}", state.state_ref().read_stack_pointer::<Endian>().unwrap());
		// ---- Periphrial Handling ----
		// Tick 
		// debug!("[CMT] tick");
		let (cmt_0, cmt_1) = &mut self.backend;
		let (int_0, int_1) = &mut self.interrupt;
		let (h_0, h_1) 		= &mut self.handler;
		cmt_0.tick();			// Suppose each pc change is one clock cycle
		cmt_1.tick();
		if !cmt_0.is_matched() && !int_0.is_triggered() && !cmt_1.is_matched() && !int_0.is_triggered(){
			return Ok(HookStepAction::Pass.into());
		}

		// If the interrupt is not enabled, then continue execution
		if !int_0.is_enabled() && !int_0.is_triggered() && !int_1.is_enabled() && !int_1.is_triggered(){
			return Ok(HookStepAction::Pass.into());
		}

		if let backend::InterruptHandler::Override(_o) = h_0 {
			panic!("Interrupt Handler Rust Override not supported in this periphrial yet");
		}
		if let backend::InterruptHandler::Override(_o) = h_1 {
			panic!("Interrupt Handler Rust Override not supported in this periphrial yet");
		}
		
		const INST_RTE: u32 = 0b0000_00000_0010_1011u32;
		let instruction = Endian::read_u32(state.state_ref().view_values(address, 4).unwrap());
		// From this point, the interrupt has been triggered do Interrupt Handling
		if int_0.is_triggered() {
			if instruction == INST_RTE{
				// return from interrupt disable triggered status
				info!("[CMT0] Return from interrupt");
				int_0.set_triggered(false);
			}
			// If we are in a interrupt routine, then do not branch to interrupt again
			return Ok(HookStepAction::Pass.into());
		}
		if cmt_0.is_matched() && cmt_0.is_enabled(){
			int_0.set_triggered(true);
		}

		// From this point, the interrupt has been triggered do Interrupt Handling
		if int_1.is_triggered() {
			if instruction == INST_RTE{
				// return from interrupt disable triggered status
				info!("[CMT1] Return from interrupt");
				int_1.set_triggered(false);
			}
			// If we are in a interrupt routine, then do not branch to interrupt again
			return Ok(HookStepAction::Pass.into());
		}
		if cmt_1.is_matched() && cmt_1.is_enabled(){
			int_1.set_triggered(true);	
		}
		
		// ---- Interrupt Handling ----
		// TODO: Get pending interrupt list with priority
		// TODO: Add push stack to utils

		// Fetch the routine start address from hte handling vector table
		let routine_addr = if int_0.is_triggered() {
			h_0.get_routine_address::<_, Endian>(state).unwrap()
		} else if int_1.is_triggered(){
			h_1.get_routine_address::<_, Endian>(state).unwrap()
		} else {
			return Ok(HookStepAction::Pass.into());	
		};

		info!("[CMT] Interrupt Triggered, jump to {}", routine_addr);

		// Stack Operation
		let state_mut = state.state_mut();
		let mut sp = state_mut.stack_pointer_value().unwrap();

		// Save SR to the stack, copy priority level of accepted interrupt to I3-I0 in SR
		let sr_reg = state_mut.registers().register_by_name("sr").unwrap();
		let sr_val: u32 = state_mut.get_operand(&sr_reg.into()).unwrap();

		sp = sp - Address::from(4u32);		// Push Stack

		Endian::write_u32(state_mut.view_values_mut(sp, 4).unwrap(), sr_val.clone());

		// Save PC to stack
		let pc: u32 = address.try_into().unwrap();
		sp = sp - Address::from(4u32);		// Push Stack
		Endian::write_u32(state_mut.view_values_mut(sp, 4).unwrap(), pc.clone());

		// Write stack_pointer
		state_mut.set_stack_pointer_value(sp).unwrap();

		// Jump to the routine start address (non-delay branch)
		return Ok(HookStepAction::Branch((1, routine_addr)).into());
		// return Ok(HookStepAction::Branch(routine_addr).into());

		
    }

	
	fn hook_memory_read(&mut self, state: &mut Self::State, address: &Address, _size: usize) -> Result<HookOutcome<HookAction<Self::Outcome>>, HookError<Self::Error>> {

        let (min, max) = self.address_range;
		let addr_u32 : u32 = address.try_into().unwrap();
        if min<= *address && *address<= max {
			info!("[CMT] read from reg {}", address);
			// Handle read from reg
			let (cmt_0, cmt_1) = &mut self.backend;
			if addr_u32 >= 0xfffec000 && addr_u32 <= 0xfffec006{
				cmt_0.handle_reg_read::<_, Endian>(state, addr_u32 as u64);
			} 
			
			if addr_u32 == 0xfffec000 || (addr_u32 >= 0xfffec008 && addr_u32 <= 0xfffec00c){ 
				cmt_1.handle_reg_read::<_, Endian>(state, addr_u32 as u64);
			}
        }

		// IPR10 (7 to 4) & IPR10 (3 to 0)
        Ok(HookAction::Pass.into())
    }

    fn hook_memory_write(&mut self, state: &mut Self::State, address: &Address, _size: usize, value: &[u8]) -> Result<HookOutcome<HookAction<Self::Outcome>>, HookError<Self::Error>> {

        let (min, max) = self.address_range;
		let addr_u32 : u32 = address.try_into().unwrap();


		// let write_val = u32::from_be_bytes(value.try_into().unwrap());	
        if min<= *address && *address <= max {
			// TODO: Check endian
			let write_val = match value.len(){
				1 => {u8::from_be_bytes(value.try_into().unwrap()) as u32},
				2 => {u16::from_be_bytes(value.try_into().unwrap()) as u32},
				4 => {u32::from_be_bytes(value.try_into().unwrap()) as u32},
				_ => {panic!("Unexpected value size")}
			};
			info!("[CMT] write to reg {}, val: {:?}", address, value);
			// Handle write to reg

			let (cmt_0, cmt_1) = &mut self.backend;
			let (int_0, int_1) = &mut self.interrupt;
			if addr_u32 >= 0xfffec000 && addr_u32 <= 0xfffec006{
				cmt_0.handle_reg_write::<_, Endian>(state, addr_u32 as u64, write_val);
				if cmt_0.is_interrupt_enabled() {
					int_0.set_enable(true);
				} else {
					int_0.set_enable(false);
				}
			}

			if addr_u32 == 0xfffec000 || (addr_u32 >= 0xfffec008 && addr_u32 <= 0xfffec00c){ 
				cmt_1.handle_reg_write::<_, Endian>(state, addr_u32 as u64, write_val);
				if cmt_1.is_interrupt_enabled() {
					int_1.set_enable(true);
				} else {
					int_1.set_enable(false);
				}
			}
        }
        Ok(HookAction::Pass.into())
    }



}


impl<S: 'static, E> ClonableHookConcrete for CompareMatchTimer<S, E>
where S: AsState<PCodeState<u8, Endian>>,
	E: std::error::Error +  Send + Sync + 'static
    { }