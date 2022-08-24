use std::collections::HashMap;

use fugue::ir::{
    Address,
};
use fuguex::state::{
	AsState,
    pcode::PCodeState,
	StateOps,
};
use fugue::bytes::{Order};

use std::convert::TryInto;


// Note: when using as underflow/overflow timer
// The under/overflow is compared against the counter
// If matched and counting forward, then overflow, 
// if counting backwards, via versa

#[derive(Clone, Debug)]
pub struct CompareMatchTimer {
	counter_start: bool,
	current_tick: u128,
	compare_against: u128,
	count_forward: bool, 	// Optional: 1 for forward counting, 0 for backward counting
	flag_overflow: bool,	// If counter was overflowed
	flag_underflow: bool, 	// If counter was underflowed
	matched: bool,
	reset_on_match: bool,
	interrupt_enabled: bool,
	match_toggle: bool,
	reg_read_map: HashMap<u64, HashMap<u32, FunName>>, // reg_addr: (mask: mapped_function_name)
	reg_write_map: HashMap<u64, HashMap<u32, FunName>>,// reg_addr: (mask: mapped_function_name)
}

impl Default for CompareMatchTimer {
	fn default() -> Self {
		Self{
			counter_start: false,
			current_tick: 0,
			compare_against: 0,
			count_forward: true,
			flag_overflow: false,
			flag_underflow: false,
			matched: false,
			match_toggle: false,
			reset_on_match: true,
			interrupt_enabled: false,		// Enable or disable interrupt generation
			reg_read_map: HashMap::new(),
			reg_write_map: HashMap::new(),
		}
	}
	
}
#[derive(Clone, Debug)] 
#[allow(non_camel_case_types)]
pub enum FunName {
	is_enabled,
	is_interrupt_enabled,
	is_matched,
	get_compare_against,
	get_current_tick,
	set_enable,
	set_interrupt_enabled,
	set_compare_against,
	clear_matched_flag,
	set_current_tick,
	set_count_forward,
	get_count_forward_flag,
	get_flag_underflow,
	get_flag_overflow,
	get_flag_overunderflow,	
	set_flag_overunderflow,
	set_match_toggle,
	get_match_toggle,
	
}



impl CompareMatchTimer {

	pub fn is_enabled(&self) -> bool{
		self.counter_start
	}

	pub fn set_enable(&mut self, val: bool) {
		self.counter_start = val;
	}

	pub fn is_interrupt_enabled(&self) -> bool {
		self.interrupt_enabled
	}

	pub fn set_interrupt_enabled(&mut self, val: bool) {
		self.interrupt_enabled = val;
	}

	// Set if reset the counter when matched
	// True: reset, false: don't reset
	pub fn config_reset_on_match(&mut self, val: bool){
		self.reset_on_match = val;
	}

	pub fn set_compare_against(&mut self, val: u128){
		self.compare_against = val;
	}

	pub fn get_compare_against(&self) -> u128{
		self.compare_against
	}

	pub fn set_count_forward(&mut self, val: bool) {
		self.count_forward = val;
	}

	pub fn get_count_forward_flag(&self) -> bool {
		self.count_forward
	}

	pub fn get_flag_underflow(&self) -> bool {
		self.flag_underflow
	}
	pub fn get_flag_overflow(&self) -> bool {
		self.flag_overflow
	}

	pub fn set_flag_overflow(&mut self, val: bool){
		self.flag_overflow = val; 
	}

	pub fn set_flag_underflow(&mut self, val: bool){
		self.flag_underflow = val;
	}
	pub fn set_flag_underoverflow(&mut self, val: bool){
		self.flag_underflow = val;
		self.flag_overflow = val;
	}

	pub fn get_flag_overunderflow(&self) -> bool {
		self.flag_overflow | self.flag_overflow
	}

	pub fn set_match_toggle(&mut self, val:bool) {
		self.match_toggle = val;
	}

	pub fn get_match_toggle(&self) -> bool {
		self.match_toggle
	}

	// return true if matched in enabled condition
	pub fn tick(&mut self) -> bool{
		if self.counter_start == false {
			return false;
		} 
		// Count according to counting direction
		if self.count_forward {
			self.current_tick += 1;
		} else {
			self.current_tick -= 1;	
		}

		if self.current_tick == self.compare_against {
			self.matched = true;
			self.match_toggle = !self.match_toggle;
			// Reset the counter
			if self.reset_on_match {
				self.current_tick = 0;
			}

			return true;
		} else {
			return false;
		}
	}

	pub fn get_current_tick(&self) -> u128 {
		self.current_tick
	}

	pub fn set_current_tick(&mut self, val: u128){
		self.current_tick = val;
	}

	pub fn is_matched(&self) -> bool {
		if self.is_enabled() {
			self.matched
		} else {
			false
		}
	}

	pub fn clear_matched_flag(&mut self){
		self.matched = false;
	}

	pub fn map_function_addr_read(&mut self, addr: u64, mask: u32, fun_name: &FunName){
		// let mask_fun = self.reg_read_map.get_mut(&addr);
		let exist = self.reg_read_map.contains_key(&addr);
		if exist {
			let v = self.reg_read_map.get_mut(&addr).unwrap();
			// If the hashmap under this address exists
			// Directly insert the mask: fun_name
			v.insert(mask, fun_name.clone());
		} else {
			let mut v = HashMap::new();
			v.insert(mask, fun_name.clone());
			self.reg_read_map.insert(addr, v);
		}
	}

	pub fn map_function_addr_write(&mut self, addr: u64, mask: u32, fun_name: &FunName){
		// let mask_fun = self.reg_read_map.get_mut(&addr);
		let exist = self.reg_write_map.contains_key(&addr);
		if exist {
			let v = self.reg_write_map.get_mut(&addr).unwrap();
			// If the hashmap under this address exists
			// Directly insert the mask: fun_name
			v.insert(mask, fun_name.clone());
		} else {
			let mut v = HashMap::new();
			v.insert(mask, fun_name.clone());
			self.reg_write_map.insert(addr, v);
		}
	}

	#[inline(always)]
	fn set_bits_bool(val: u32, mask: u32, set: bool) -> u32 {
		// When set is true, set the masked to the val
		// When set is false, clear the masked bits
		let mut val = val;
		if set {
			val = val | mask;
		} else {
			val = val & (!mask);
		}

		val
	}
	#[inline(always)]
	fn get_mask_start_bit(mask: u32) -> Option<(u8, u8)>{
		// Return the index of start and end bit of the mask
		let mut start: i16 = -1;
		let mut end: i16 = -1;
		for i in 0..32 {
			if start == -1 && (mask & (0x1 << i) != 0x0) {
				start = i;
			}

			if start != -1 && mask & (0x1 << i) == 0x0 {
				end = i-1;
				if i == 31 {
					end = 31;
				}
				break;
			}
		}
		if start >= 0 && end > 0 {
			Some((start as u8, end as u8))
		} else {
			None
		}
	}

	#[inline(always)]
	fn set_bits_val(val: u32, mask: u32, set_val: u32) -> u32 {
		let mut val = val;
		if let Some((s, _e)) = Self::get_mask_start_bit(mask){
			val = val & !mask; 					// Clear the target bits
			val |= (set_val << s) & mask;		// Set the target bits
		} else {
			panic!("mask error, mask 0x{:X}", mask);
		}	
		val
	}

	// Only support 32 bit reg size for now
	// Return the value of the address
	pub fn handle_reg_read<S: AsState<PCodeState<u8, E>>, E: Order>(&mut self, state: &mut S, addr: u64) -> u32{
		let mask_fun_map = self.reg_read_map.get(&addr).expect(&format!("Addr: 0x{:x} is not bind to any function", addr));

		let mut val: u32 = E::read_u32(state.state_ref().view_values(Address::from(addr), 4).unwrap());

		// Apply all operations under this address
		for (mask, fun) in mask_fun_map {
			match fun {
				FunName::is_enabled 			=> {val = Self::set_bits_bool(val, *mask, self.is_enabled());},
				FunName::is_interrupt_enabled 	=> {val = Self::set_bits_bool(val, *mask, self.is_interrupt_enabled());},
				FunName::is_matched				=> {val = Self::set_bits_bool(val, *mask, self.is_matched());},
				FunName::get_compare_against	=> {val = Self::set_bits_val(val, *mask, self.get_compare_against().try_into().unwrap());},
				FunName::get_current_tick 		=> {val = Self::set_bits_val(val, *mask, self.get_current_tick().try_into().unwrap());},
				FunName::get_count_forward_flag => {val = Self::set_bits_bool(val, *mask, self.get_count_forward_flag());},
				FunName::get_flag_overflow		=> {val = Self::set_bits_bool(val, *mask, self.get_flag_overflow());},
				FunName::get_flag_underflow		=> {val = Self::set_bits_bool(val, *mask, self.get_flag_underflow());},
				FunName::get_flag_overunderflow		=> {val = Self::set_bits_bool(val, *mask, self.get_flag_overunderflow());},
				FunName::get_match_toggle		=> {val = Self::set_bits_bool(val, *mask, self.get_match_toggle());}, 
				_ => { panic!("{:?} mapping not supported in compare_match_timer", fun)}
			}
		}
		// Write Value at the address
		let mut value_tmp = [0u8; 4];
		E::write_u32(&mut value_tmp, val);
		state.state_mut().set_values(Address::from(addr),  &value_tmp).unwrap();
		val
	}

	pub fn handle_reg_write <S: AsState<PCodeState<u8, E>>, E: Order>(&mut self, _state: &S, addr: u64, write_val: u32){
		// Change the periprial state according to memory write
		let mask_fun_map = self.reg_write_map.get(&addr).unwrap().clone();

		// Apply all operations under this address
		for (mask, fun) in mask_fun_map {

			// Match mask
			if write_val & mask != 0 {
				// Setting bit condition
				match fun{
					FunName::set_enable 				=> {self.set_enable(true);},
					FunName::set_interrupt_enabled 		=> {self.set_interrupt_enabled(true);},
					FunName::set_compare_against		=> {
							self.set_compare_against(write_val as u128);
						// if mask != 0xffffffff {
						// 	// Usually the whold regisiter if for the counter
						// 	// If not, need to be implemented seperately
						// 	panic!("The mask of compare against is not for full reg");
						// } else {
						// }
					},
					FunName::clear_matched_flag			=> {/* Do nothing if FW is trying to set the flag*/},
					FunName::set_current_tick			=> {self.set_current_tick(write_val as u128);},
					FunName::set_count_forward			=> {self.set_count_forward(true)},
					FunName::set_match_toggle			=> {self.set_match_toggle(true)},
					_ => { panic!("{:?} mapping not supported in compare_match_timer", fun);} 
				}
			} else {
				// Clearing bit condition
				match fun{
					FunName::set_enable 				=> {self.set_enable(false);},
					FunName::set_interrupt_enabled 		=> {self.set_interrupt_enabled(false);},
					FunName::set_compare_against		=> {self.set_compare_against(0);},
					FunName::set_current_tick			=> {self.set_current_tick(0);},
					FunName::clear_matched_flag 		=> {self.clear_matched_flag();},
					FunName::set_count_forward			=> {self.set_count_forward(false)}
					FunName::set_match_toggle			=> {self.set_match_toggle(false)}
					FunName::set_flag_overunderflow		=> {self.set_flag_underoverflow(false)}
					_ => { panic!("{:?} mapping not supported in compare_match_timer", fun)}  
				}
			}
		}

	}


}


#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn tick_match_test() -> Result<(), String> {
		let mut cmt = CompareMatchTimer::default();
		cmt.set_compare_against(0x2);
		cmt.set_enable(true);

		let t1 = cmt.tick();
		let t2 = cmt.tick();

		if t1 == false && t2== true {
			Ok(())
		} else {
			Err(String::from("Tick counting error"))
		}
    }

    #[test]
	fn tick_count_test() -> Result<(), String> {
		let mut cmt = CompareMatchTimer::default();
		cmt.set_compare_against(0x2);
		cmt.set_enable(false);

		cmt.tick();

		if cmt.get_current_tick() != 0 {
			return Err(String::from("Ticked counted when not enabled"));
		}
		cmt.set_enable(true);
		cmt.tick();

		if cmt.get_current_tick() != 1 {
			return Err(String::from("Tick counting error"));
		}

		let f = cmt.tick();
		// should fired and reset now
		if f==true && cmt.get_current_tick() == 0 {
			return Ok(());
		} else {
			return Err(String::from("Tick not reset after fired"));
		}
	
	}
}
