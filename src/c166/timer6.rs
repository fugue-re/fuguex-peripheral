use std::marker::PhantomData;
use std::ops::{RangeInclusive};
use thiserror::Error;
use bitvec::prelude::*;
use intervals::collections::{IntervalSet};
use fuguex::state::{
    AsState, 
    pcode::PCodeState,
    StateOps,
};
use fugue::ir::{
    Address,
};
use fugue::bytes::{LE};
use byteorder::ByteOrder;

use crate::backend::CompareMatchTimer;

#[derive(Debug, Error)]
pub enum TimerError {
    #[error("OtherError")]
    OtherError {},

}

const T6_CONTROL: u16 = 0xff48u16;
const _T6_VALUE: u16 = 0xfe48u16;

const _T6I: RangeInclusive<usize> = 0..=2;
const T6M: RangeInclusive<usize> = 3..=5;
const T6R: usize = 6;
const T6UD: usize = 7;
const _T6UDE: usize = 8;
const _T6OE: usize = 9;
const _T6OTL: usize = 10;
const _T6SR: usize = 15;

#[repr(u8)]
pub enum T6Mode {
    Timer = 0,
    Counter = 1,
    GatedActiveLow = 2,
    GatedActiveHigh = 3,
}

#[repr(u8)]
pub enum T6Run {
    Stopped = 0,
    Running = 1,
}

#[repr(u8)]
pub enum T6UpDown {
    Up = 0,
    Down = 1,
}

#[derive(Clone)]
pub struct Timer6<S>
where
    S: AsState<PCodeState<u8, LE>>,
{
    backend: CompareMatchTimer,
    address_range: IntervalSet<Address>,
    marker: PhantomData<S>,
}

impl<S> Timer6<S>
where
    S: AsState<PCodeState<u8, LE>>,
{
    pub fn new() -> Self {
        let mut address_range = IntervalSet::new();
        address_range.insert(
            Address::from(T6_CONTROL)..=Address::from(T6_CONTROL + 1),
            (),
        );

        Self {
            backend: CompareMatchTimer::default(),
            address_range,
            marker: PhantomData,
        }
    }

    pub fn mode(&self, state: &S) -> Result<T6Mode, TimerError> {
        let val = LE::read_u16(state.state_ref().view_values(Address::from(T6_CONTROL), 2).unwrap());
        let t6m = unsafe {
            std::mem::transmute(val.view_bits::<Lsb0>()
                                .get(T6M)
                                .unwrap()
                                .iter()
                                .fold(0u8, |acc, elt| (acc << 1) | (if *elt { 1 } else { 0 })))
        };
        Ok(t6m)
    }

    pub fn up_down(&self, state: &S) -> Result<T6UpDown, TimerError> {
        let val = LE::read_u16(state.state_ref().view_values(Address::from(T6_CONTROL), 2).unwrap());
        if *val.view_bits::<Lsb0>().get(T6UD).unwrap() {
            Ok(T6UpDown::Down)
        } else {
            Ok(T6UpDown::Up)
        }
    }

    pub fn running(&self, state: &S) -> Result<T6Run, TimerError> {
        let val_bytes = state.state_ref().view_values(Address::from(T6_CONTROL), 2).unwrap();
        let val = LE::read_u16(val_bytes);
        if *val.view_bits::<Lsb0>().get(T6R).unwrap() {
            Ok(T6Run::Running)
        } else {
            Ok(T6Run::Stopped)
        }
    }
}
