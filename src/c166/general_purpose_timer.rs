use crate::backend::EmptyInterruptHandlerOverrider;
use std::marker::PhantomData;
use thiserror::Error;
use fugue::ir::{
    Address,
};
use metaemu::state::{
    AsState, 
    pcode::PCodeState,
    StateOps,
};
use metaemu::concrete::hooks::{ClonableHookConcrete, HookConcrete};
use fugue::bytes::{LE};
use metaemu::hooks::types::{HookStepAction,HookAction, HookOutcome, Error as HookError};
use metaemu::machine::StepState;
use crate::backend;
use crate::backend::compare_match_timer::FunName as CMTFunName;
use std::convert::TryInto;
use log::{info};
use metaemu::state::pcode::Error as PCodeError;

#[derive(Debug, Error)]
pub enum C166GPTError {
    #[error("OtherError")]
    OtherError {},

}

#[derive(Clone)]
pub struct GeneralPurposeTimer <S>
where
	S: AsState<PCodeState<u8, LE>>,
{
	backend_cmt6: backend::CompareMatchTimer,
	interrupt: backend::Interrupt,
	handler: backend::InterruptHandler<EmptyInterruptHandlerOverrider<S, LE>>,
	address_range: (Address, Address),
	endian: PhantomData<LE>,
}

type Endian = LE;

impl<S: AsState<PCodeState<u8, Endian>>> GeneralPurposeTimer<S> {
	pub fn new() -> Self{
		let mut cmt6 = backend::CompareMatchTimer::default();
		// const ADDR_T6IC		:u64 	= 0xff68; 

		const ADDR_T6CON	:u64 	= 0xff48;
		const ADDR_T6		:u64	= 0xfe48;

		cmt6.map_function_addr_read(ADDR_T6CON, 		0x40, 		&CMTFunName::is_enabled);
		cmt6.map_function_addr_write(ADDR_T6CON, 		0x40, 		&CMTFunName::set_enable);
		cmt6.map_function_addr_read(ADDR_T6CON, 		0x400, 		&CMTFunName::get_match_toggle);
		cmt6.map_function_addr_write(ADDR_T6CON, 		0x400, 		&CMTFunName::set_match_toggle);
		cmt6.map_function_addr_read(ADDR_T6, 			0xffff, 	&CMTFunName::get_current_tick);
		cmt6.map_function_addr_write(ADDR_T6, 			0xffff, 	&CMTFunName::set_current_tick);

		cmt6.set_compare_against(0xffff);// TODO: Check the correctness

		// cmt.map_function_addr_read(ADDR_CMCSR_0, 	0x40, 		&CMTFunName::is_interrupt_enabled);
		// cmt.map_function_addr_write(ADDR_CMCSR_0, 	0x40, 		&CMTFunName::set_interrupt_enabled);

		// cmt.map_function_addr_read(ADDR_CMCSR_0, 	0x80, 		&CMTFunName::is_matched);
		// cmt.map_function_addr_write(ADDR_CMCSR_0, 	0x80, 		&CMTFunName::clear_matched_flag);

		// cmt.map_function_addr_read(ADDR_CMCNT_0, 	0xffff, 	&CMTFunName::get_current_tick);
		// cmt.map_function_addr_write(ADDR_CMCNT_0, 	0xffff, 	&CMTFunName::set_current_tick);

		// cmt.map_function_addr_read(ADDR_CMCOR_0, 	0xffff, 	&CMTFunName::get_compare_against);
		// cmt.map_function_addr_write(ADDR_CMCOR_0, 	0xffff, 	&CMTFunName::set_compare_against);
	
		Self {
			backend_cmt6: cmt6,
			interrupt: backend::Interrupt::new("GPT"),
			handler : backend::InterruptHandler::Vector(Address::from(0x000002BCu32)), //CMT1: 000002C0 IPR10 7-4, 3-0
			address_range: (Address::from(0xfe48u32), Address::from(0xff68u32)), // TODO: make interval ?
			endian: PhantomData,
		}
	}
}

impl <S: 'static> HookConcrete for GeneralPurposeTimer<S>
where
	S: AsState<PCodeState<u8, Endian>> + StateOps
{

	type State = PCodeState<u8, Endian>;
	type Error = PCodeError;
	type Outcome = String;

	fn hook_architectural_step(&mut self, _state: &mut Self::State, _address: &Address, _operation: &StepState)
		 -> Result<HookOutcome<HookStepAction<Self::Outcome>>, HookError<Self::Error>> {
		// println!("[GPT] tick");
		self.backend_cmt6.tick();		// Assume each pc change is for 1 clock cycle
		return Ok(HookStepAction::Pass.into());
    }

	
	fn hook_memory_read(&mut self, state: &mut Self::State, address: &Address, _size: usize) -> Result<HookOutcome<HookAction<Self::Outcome>>, HookError<Self::Error>> {

        // let (min, max) = self.address_range;
		let addr_u32 : u32 = address.try_into().unwrap();
        // if min<= address && address<= max {
		if addr_u32 == 0xff48 || addr_u32 == 0xfe48{
			// Handle read from reg
			let val = self.backend_cmt6.handle_reg_read::<_, Endian>(state, addr_u32 as u64);
			info!("[GPT] read from reg {}, val: 0x{:x}", address, val);
        }

		// IPR10 (7 to 4) & IPR10 (3 to 0)
        Ok(HookAction::Pass.into())
    }

    fn hook_memory_write(&mut self, state: &mut Self::State, address: &Address, _size: usize, value: &[u8]) -> Result<HookOutcome<HookAction<Self::Outcome>>, HookError<Self::Error>> {

        // let (min, max) = self.address_range;
		let addr_u32 : u32 = address.try_into().unwrap();


		// let write_val = u32::from_be_bytes(value.try_into().unwrap());	
        // if min<= address && address <= max {
		if addr_u32 == 0xff48 || addr_u32 == 0xfe48{
			// get the length of value

			let write_val = match value.len(){
				1 => {u8::from_le_bytes(value.try_into().unwrap()) as u32},
				2 => {u16::from_le_bytes(value.try_into().unwrap()) as u32},
				_ => {panic!("Unexpected value size")},
			};
			info!("[GPT] write to reg {}, val: {:?}", address, value);
			// Handle write to reg

			self.backend_cmt6.handle_reg_write::<_, Endian>(state, addr_u32 as u64, write_val);
			if self.backend_cmt6.is_interrupt_enabled() {
				self.interrupt.set_enable(true);
			} else {
				self.interrupt.set_enable(false);
			}
        }
        Ok(HookAction::Pass.into())
    }



}


impl<S: 'static> ClonableHookConcrete for GeneralPurposeTimer<S>
where S: AsState<PCodeState<u8, LE>> + Clone + StateOps,
    { }