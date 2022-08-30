
use std::collections::LinkedList;
use socketcan::CANFrame;
use crate::polling::PollingPeripheralHandler;
use crate::polling;

use fugue::ir::{
    Address,
};
use fuguex::state::{
    AsState, 
    pcode::PCodeState, StateOps
};
use fugue::bytes::{Order};
use parking_lot::Mutex;
use std::marker::PhantomData;
use std::ops::{Deref, DerefMut};
use std::collections::{HashMap};
use thiserror::Error;

// use byteorder::{BE, LE};
use byteorder::{LE};
// use muexe_core::pcode::Operand;
// use std::convert::TryInto;
use byteorder::{WriteBytesExt, ReadBytesExt};
// use std::thread;
use std::time::Duration;
use std::convert::TryInto;

use log::{info, warn};


#[derive(Debug, Error)]
pub enum Error {
    #[error(transparent)]
    SocketCanSocket(#[from] socketcan::CANSocketOpenError),
    #[error("CAN transport error: {0}")]
    SocketCanTransport(#[from] std::io::Error),
    #[error("CAN frame construction error: {0}")]
    SocketCanFrame(#[from] socketcan::ConstructionError),
    #[error("CAN: Can not get regisiter with name: {0}")]
    RSCanReg(String),
    #[error("CAN: not in vcan mode, should not call connect")]
    RSCanNotVCAN(),
}

impl From<Error> for polling::Error {
    fn from(error: Error) -> polling::Error {
        polling::Error::HandlerError("RSCan-peripheral".to_string() + format!("{:?}",error).as_str() )
    }
}

// CanSocket
pub struct CanSocket<'a>(parking_lot::MutexGuard<'a, socketcan::CANSocket>);
impl<'a> Deref for CanSocket<'a> {
    type Target = socketcan::CANSocket;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}
impl<'a> DerefMut for CanSocket<'a> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}

// RSCan
// #[derive(Debug, Clone)]
// enum RSCanState{
//     IDLE,
// }

// pub struct RSCanRegs {
//     // const BASE:u32 = 0xffd00000u32;
//     // let base = 0xffd00000u32;
//     // let prefix = String::from("RSCAN0");
//     CFPCTR0  : Address,
//     CFIDk    : Address,
//     CFPTRk   : Address,
//     CFDF0k   : Address,
//     CFDF1k   : Address,
//     TXQPCTR0 : Address,   
//     TMCp     : Address,
//     TMIDp    : Address,
//     TMPTRp   : Address,
//     TMDF0p   : Address,
//     TMDF1p   : Address,
//     TMSTSp   : Address,
//     RMND0    : Address,
//     RMIDq    : Address,
//     RMPTRq   : Address,
//     RMDF0q   : Address,
//     RMDF1q   : Address
// }

// impl RSCanRegs{
//     fn new(base : u32) -> Self{
//         Self{
//             CFPCTR0  : Address::from(base + 0x01d8)         ,    // For FIFO Transfer/Receive
//             CFIDk    : Address::from(base + 0x0e80 + 0*0x10),
//             CFPTRk   : Address::from(base + 0x0e84 + 0*0x10),
//             CFDF0k   : Address::from(base + 0x0e88 + 0*0x10),
//             CFDF1k   : Address::from(base + 0x0e8C + 0*0x10),
//             TXQPCTR0 : Address::from(base + 0x03e0 + 0*4)   ,    // For Transmit Queue Transfer  
//             TMCp     : Address::from(0xffc71004u32)         ,    // For Transmission Buffer 
//             TMIDp    : Address::from(base + 0x1000 + 0*0x10),
//             TMPTRp   : Address::from(base + 0x1004 + 0*0x10),
//             TMDF0p   : Address::from(base + 0x1008 + 0*0x10),
//             TMDF1p   : Address::from(base + 0x100c + 0*0x10),
//             TMSTSp   : Address::from(base + 0x02d0 + 0*0x01),
//             RMND0    : Address::from(base + 0x00A8 + 0*0x4) ,    // For Receive Buffer Reading
//             RMIDq    : Address::from(base + 0x0600 + 0*0x10),
//             RMPTRq   : Address::from(base + 0x0604 + 0*0x10),
//             RMDF0q   : Address::from(base + 0x0608 + 0*0x10),
//             RMDF1q   : Address::from(base + 0x060c + 0*0x10),
//         }
//     }
// }
// #[derive(Debug)]
// pub struct CANFrame {
//     can_id : u32,   // only lower 11bits/29bits are used
//     data:   u64,
// }

#[derive(Debug)]
pub struct RSCan<S, O> 
    where S: AsState<PCodeState<u8, O>> + StateOps,
          O: Order
{ 
    socket: Option<Mutex<socketcan::CANSocket>>,
    interface: String,
    state: PhantomData<S>,
    regisiters: HashMap<String, Address>,
    data_queue: LinkedList<CANFrame>,
    select_vcan_mode: bool,
    order: PhantomData<O>,
}

impl<S, O> Default for RSCan<S, O> 
    where S: AsState<PCodeState<u8, O>> + StateOps,
          O: Order
{
    fn default() -> Self {
        Self{
            interface: String::from("N/A"),
            socket: None,
            state: PhantomData,
            regisiters : Self::get_peripheral_regs(),
            data_queue: LinkedList::new(),
            select_vcan_mode: true,
            order: PhantomData
        }
    }

}

impl<S, O> Clone for RSCan<S, O> 
    where S: AsState<PCodeState<u8, O>> + StateOps,
          O: Order
{
    fn clone(&self) -> Self {
        // since we can't report an error here, we first attempt to open
        // a connection, if we can't then we can try later.
        Self {
            interface: self.interface.clone(),
            socket: socketcan::CANSocket::open(&self.interface).ok().map(Mutex::new),
            state: PhantomData,
            regisiters: self.regisiters.clone(),
            data_queue: LinkedList::new(),
            select_vcan_mode: self.select_vcan_mode,
            order: PhantomData
        }
    }
}


impl<S, O> RSCan<S, O> 
    where S: AsState<PCodeState<u8, O>> + StateOps,
            O: Order 
{
    fn get_peripheral_regs() -> HashMap<String, Address>{
        let mut regs = HashMap::new();
        
        const BASE:u32 = 0xffd00000u32;
        // let base = 0xffd00000u32;
        // let prefix = String::from("RSCAN0");
        regs.insert( String::from("CFPCTR0")  ,Address::from(BASE + 0x01d8)         );    // For FIFO Transfer/Receive
        regs.insert( String::from("CFID0")    ,Address::from(BASE + 0x0e80 + 0*0x10));
        regs.insert( String::from("CFPTR0")   ,Address::from(BASE + 0x0e84 + 0*0x10));
        regs.insert( String::from("CFDF00")   ,Address::from(BASE + 0x0e88 + 0*0x10));
        regs.insert( String::from("CFDF10")   ,Address::from(BASE + 0x0e8C + 0*0x10));
        regs.insert( String::from("C0STS")    ,Address::from(BASE + 0x0008 + 0*0x10));
        regs.insert( String::from("C0ERFL")   ,Address::from(BASE + 0x000C + 0*0x10));
        regs.insert( String::from("TXQPCTR0") ,Address::from(BASE + 0x03e0 + 0*4)   );    // For Transmit Queue Transfer   
        regs.insert( String::from("TMC0")     ,Address::from(BASE + 0x0250 + 0*1)   );    // For Transmission Buffer 
        regs.insert( String::from("TMID0")    ,Address::from(BASE + 0x1000 + 0*0x10));
        regs.insert( String::from("TMPTR0")   ,Address::from(BASE + 0x1004 + 0*0x10));
        regs.insert( String::from("TMDF00")   ,Address::from(BASE + 0x1008 + 0*0x10));
        regs.insert( String::from("TMDF10")   ,Address::from(BASE + 0x100c + 0*0x10));
        regs.insert( String::from("TMSTS0")   ,Address::from(BASE + 0x02d0 + 0*0x01));
        regs.insert( String::from("RMND0")    ,Address::from(BASE + 0x00A8 + 0*0x4) );    // For Receive Buffer Reading
        regs.insert( String::from("RMID0")    ,Address::from(BASE + 0x0600 + 0*0x10));
        regs.insert( String::from("RMPTR0")   ,Address::from(BASE + 0x0604 + 0*0x10));
        regs.insert( String::from("RMDF00")   ,Address::from(BASE + 0x0608 + 0*0x10));
        regs.insert( String::from("RMDF10")   ,Address::from(BASE + 0x060c + 0*0x10));

        regs.insert( String::from("RFID0")    ,Address::from(BASE + 0x0e00 + 0*0x10));
        regs.insert( String::from("RFSTS0")    ,Address::from(BASE + 0x00d8 + 0*4));
        regs.insert( String::from("RFDF00")    ,Address::from(BASE + 0x0e08 + 0*0x10));
        regs.insert( String::from("RFDF10")    ,Address::from(BASE + 0x0e0c + 0*0x10));
        regs.insert( String::from("RFPTR0")    ,Address::from(BASE + 0x0e04 + 0*0x10));
        regs.insert( String::from("RFPCTR0")   ,Address::from(BASE + 0x00f8 + 0*0x04));

        regs.insert( String::from("GSTS")    ,Address::from(BASE + 0x008c));
        return regs;
    }

    pub fn new<I: AsRef<str>>(virtual_can_interface: I) -> Result<Self, Error> {
        let interface = virtual_can_interface.as_ref().to_owned();
        
        // TODO: initialize memory
        let mut slf = Self {
            interface,
            socket: None,
            state: PhantomData,
            regisiters : Self::get_peripheral_regs(),
            data_queue: LinkedList::new(),
            select_vcan_mode: true,
            order: PhantomData
        };
        // only connect to socket in vcan mode
        slf.connect()?;
        Ok(slf)
    }

    pub fn new_queued() -> Result<Self, Error> {
        // TODO: initialize memory
        let slf = Self {
            interface: String::from("N/A"),
            socket: None,
            state: PhantomData,
            regisiters : Self::get_peripheral_regs(),
            data_queue: LinkedList::new(),
            select_vcan_mode: false,
            order: PhantomData
        };
        Ok(slf)
    }

    pub fn connect<'a>(&'a mut self) -> Result<CanSocket<'a>, Error> {
        match self.socket {
            None => {
                self.socket = Some(Mutex::new(socketcan::CANSocket::open(&self.interface)?));
                Ok(CanSocket(self.socket.as_mut().unwrap().lock()))
            },
            Some(ref mut socket) => Ok(CanSocket(socket.lock())),
        }
    }

    /// CANID: only lower 11bits/29bits are used
    pub fn enqueue_can_msg(&mut self, can_id: u32, data: u64) -> Result<(), &str>{
        if self.select_vcan_mode {
            return Err("push CAN msg not supported in vcan mode");
        } else {
            let mut data_tmp = [0u8; 8];
            O::write_u64(&mut data_tmp, data);
            self.data_queue.push_back(CANFrame::new(can_id, &data_tmp, false, false).unwrap());
            return Ok(());
        }
    }

    pub fn dequeue_can_msg(&mut self) -> Option<CANFrame> {
        self.data_queue.pop_front()
    }

    pub fn clear_can_msg_queue(&mut self) {
        self.data_queue.clear();
    }

    pub fn peek_can_msg(&self) -> Option<&CANFrame> {
        self.data_queue.front().clone()
    }

    pub fn get_reg_val(&self, state: &PCodeState<u8, O>, name: &str) -> Result<u32, Error>{
        let reg_addr = self.regisiters.get(name).unwrap();
        let reg_val : u32 = O::read_u32(state.view_values(*reg_addr, 4).unwrap());

        return Ok(reg_val);
    }

    // value: u8 in LE
    pub fn set_reg_value_u8(&self, state: &mut PCodeState<u8, O>, name: &str, value: u8)-> (){
        let reg_addr = self.regisiters.get(name).unwrap();
        state.set_values(*reg_addr, &[value]).unwrap();
        return ();
    }

    pub fn get_reg_addr(&self, name: &str) -> Result<&Address, Error>{
        let addr = self.regisiters.get(name).unwrap();

        return Ok(addr);
    }

    pub fn get_regs (&self) -> &HashMap<String, Address>{
        return &self.regisiters;
    }

    pub fn get_regs_range (&self) -> (Address, Address){
        let mut max = Address::from(0u64);
        let mut min = Address::from(0xFFFFFFFFFFFFFFFFu64);
        for (_key, value) in &self.regisiters{
            if *value > max{
                max = value.clone();
            }
            if *value < min{
                min = value.clone();
            }
        };
        return (Address::from(0xffd00000u32), Address::from(0xffd00000u32 + 0x19fc));
        // return (min, max);
    }

}

// Implement as Polling Peripheral
impl<S, O> PollingPeripheralHandler for RSCan<S, O> 
where S: AsState<PCodeState<u8, O>> + StateOps,
      O: Order
{
    type Input = Address;
    type Output = Address;
    type Order = O;

    fn init(&mut self, state: &mut PCodeState<u8, O>) -> std::result::Result<(), polling::Error>{
        // Init CAN regs to default values
        let addr_tmsts = self.get_reg_addr("TMSTS0").unwrap();
        state.state_mut().set_values(*addr_tmsts, &[0x00u8]).unwrap();
        
        // TODO: use another thread for listensing to socketcan and populate a queue
        // thread::spawn(move || {
        //     println!("In thread");
        // });
        
        
        // Set Receive FIFO Buffer Empty status and set everythings else as normal
        // let rfsts = self.get_reg_val(state.state_mut(), "RFSTS0").unwrap();
        let addr_rfsts = self.get_reg_addr("RFSTS0").unwrap();      
        let mut val_tmp = [0u8; 4];
        O::write_u32(&mut val_tmp, 0x01);
        state.state_mut().set_values(*addr_rfsts, &val_tmp).unwrap();

        // Init gloabl status reg
        O::write_u32(&mut val_tmp, 0x00);
        state.state_mut().set_values(*addr_rfsts, &val_tmp).unwrap();
        let addr_gsts = self.get_reg_addr("GSTS").unwrap();
        state.state_mut().set_values(*addr_gsts, &val_tmp).unwrap();



        // Create CAN interface
        return Ok(());
    }
    // Handle firmware reading from address
    // Peripheral -> Firmware
    fn handle_input(&mut self, state: &mut PCodeState<u8, O>, input: &Self::Input, _size: usize) -> std::result::Result<(), polling::Error> {
        let mut val_tmp = [0u8; 4];

        if input == self.get_reg_addr("TMSTS0").unwrap(){
            let value_var = state.state_ref().view_values(*input, 4).unwrap();
            info!("Reading from TMSTS0, value {:?}", value_var);
        } else if input == self.get_reg_addr("C0STS").unwrap(){
            let reg_addr = self.regisiters.get("C0STS").unwrap();
            let reg_val = 0x80u32;          // Communication is ready
            O::write_u32(&mut val_tmp, reg_val);
            state.state_mut().set_values(*reg_addr, &val_tmp).unwrap();
        } else if input == self.get_reg_addr("C0ERFL").unwrap() {
            let reg_addr = self.regisiters.get("C0ERFL").unwrap();
            O::write_u32(&mut val_tmp, 0x0u32);
            state.state_mut().set_values(*reg_addr, &val_tmp).unwrap();
        }
        else if input == self.get_reg_addr("RFSTS0").unwrap(){
            let msg_counter: u8 = self.data_queue.len().try_into().expect("Too many messages in the CAN data queue");
            let reg_addr = self.regisiters.get("RFSTS0").unwrap();
            let mut reg_val: u32 = O::read_u32(state.state_ref().view_values(*reg_addr, 4).unwrap());
            if msg_counter > 0 {
                reg_val = reg_val & 0xFFFF00FE;     // Clear RFEMP bits and RFMC bits indicate there are unread message
                reg_val = reg_val | ((msg_counter as u32) << 8);    // Update RFMC bits with the number of unread message
            } else {
                // Empty 
                reg_val = 0x01;
            }
            O::write_u32(&mut val_tmp, reg_val); 
            state.state_mut().set_values(*reg_addr, &val_tmp).unwrap(); 
            let value = state.state_ref().view_values(*input, 4).unwrap();
            info!("Reading from RFSTS0, Number of unread CAN msg: {} , orgi val: {:?}, changed: {:X}", msg_counter, value, reg_val);  //????? value updated after its read, so the result is not changing
        } else if input == self.get_reg_addr("RFPTR0").unwrap() {
            // Fill in DLC(Data Length) Data, Label Data and Timestamp Data
            let last_can_msg = self.data_queue.front().expect("CAN msg queue is empty"); 
            let data_len: u8 = last_can_msg.data().len().try_into().unwrap();   // Get Data length from the CANFrame
            let timestamp: u16 = 0x0;        // TODO: generate 16bit timestamp for CAN msg
            let reg_val = ((data_len as u64) << 28) as u32 | timestamp as u32;
            // Write the value back
            let reg_addr = self.regisiters.get("RFPTR0").unwrap();
            O::write_u32(&mut val_tmp, reg_val); 
            state.state_mut().set_values(*reg_addr, &val_tmp).unwrap();  
            info!("Reading from RFPTR0, returning 0x{:08x}", reg_val);
        } else if input == self.get_reg_addr("RFID0").unwrap() {
            // if code is reading RFID then the code is preparing to read can data, so we populate ID and Data

            let can_frame = if self.select_vcan_mode {
                info!("Reading RFID0 populating CAN DATA from socketcan");
                // Handle CAN Receive Buffer
                let socket = self.connect()?;

                socket.set_read_timeout(Duration::from_secs(1)).unwrap();
                let can_frame = socket.read_frame().unwrap();
                drop(socket);
                can_frame
            } else {
                *self.peek_can_msg().expect("Queue is empty")
            };

            info!("CAN id read from socketcan/queue {:?}", can_frame);
            let can_id = can_frame.id();
            // let mut _can_data = can_frame.data();

            // Write canID to RFID0
            let addr_rfid0 = self.get_reg_addr("RFID0").unwrap();
            if can_id & !0x7FFu32 > 0 {
                // Extended ID
                O::write_u32(&mut val_tmp, 0x8000000u32); 
                state.state_mut().set_values(*addr_rfid0, &val_tmp).unwrap();
            } else {
                // Normal ID
                O::write_u32(&mut val_tmp, 0x0000000u32 | (can_id & 0x7FFu32)); 
                state.state_mut().set_values(*addr_rfid0, &val_tmp ).unwrap();
            }

          
        } else if (self.get_reg_addr("RFDF00").unwrap() <= input) && (input < &(*(self.get_reg_addr("RFDF10").unwrap())  + Address::from(4u32)) ) {
            let can_frame = if self.select_vcan_mode {
                info!("Reading RFID0 populating CAN DATA from socketcan");
                // Handle CAN Receive Buffer
                let socket = self.connect()?;

                socket.set_read_timeout(Duration::from_secs(1)).unwrap();
                let can_frame = socket.read_frame().unwrap();
                drop(socket);
                can_frame
            } else {
                *self.peek_can_msg().expect("Queue is empty")
            };
            let mut can_data = can_frame.data().clone();
            // Write data to RFDF00 (Lower 4 bytes) and RFDF10 (Higher 4 bytes)
            let addr_rfdf00 = self.get_reg_addr("RFDF00").unwrap();
            let mut val_tmp_64 = [0u8; 8];
            O::write_u64(&mut val_tmp_64, can_data.read_u64::<LE>().unwrap());
            state.state_mut().set_values(*addr_rfdf00, &val_tmp_64).unwrap();  
            info!("Reading RFDF00 or RFDF10, populating data: {:?}", can_frame.data());
        } else {
            warn!("Reading from RSCAN address {} have not been implemented yet", input);
        }

        // let socket = self.connect()?;
        // let frame = socket.read_frame().map_err(Error::SocketCanTransport)?;
        // println!("IN: {:#?}", frame);
        Ok(())
    }

    // Handle firmware writting to address
    // TODO: Check size of data
    // Firmware -> Peripheral
    fn handle_output(&mut self, state: &mut PCodeState<u8, O>, output: &Self::Output, value: &[u8], _size: usize) -> std::result::Result<(), polling::Error> {
        // Handle clear transmit buffer status
        if output == self.get_reg_addr("TMSTS0").unwrap() {
            info!("Writting to TMSTS0, value: {:?}", value);
        } else if output >= self.get_reg_addr("TMDF00").unwrap() && output < &(*(self.get_reg_addr("TMDF10").unwrap()) + Address::from(4u32) ){
            info!("writting to TMDF00/10");
        } else if output == self.get_reg_addr("TMC0").unwrap() {
            info!("writting to TMC0");
            let tmc_value = value[0];
            
            if tmc_value & 0x01 != 0 {
                
                // Read TMID for ID
                let tmid : u32 = self.get_reg_val(state.state_ref(), "TMID0").unwrap();
                let mut to_id : u32 = tmid & 0x1FFFFFFF;
                if tmid & 0x80000000 != 0 {
                    // Extended ID
                }else {
                    // Standard ID
                    to_id = tmid & 0x7FF;
                }

                // Read TMPTR for length
                let tmptr : u32 = O::read_u32(state
                    .state_ref().view_values(*self.get_reg_addr("TMPTR0").unwrap(), 4).unwrap());
                // get data len from tmptr
                let data_len = (tmptr & 0xF0000000u32) >> 28;

                // get data from tmdf0 and tmdf1
                let datal : u64= O::read_u64(state.state_ref()
                    .view_values(*self.get_reg_addr("TMDF00").unwrap(), 4).unwrap());
                let datah : u64= O::read_u64(state.state_ref()
                    .view_values(*self.get_reg_addr("TMDF10").unwrap(), 4).unwrap());
                let data :u64 = datal | (datah<<32);
                // convert to vector
                let mut data_slice = vec![];
                data_slice.write_u64::<LE>(data).unwrap();

                info!("Sending CAN Data: 0x{:08x}, len: {} ", data, data_len );
                
                // clear the bit
                self.set_reg_value_u8(state.state_mut(), "TMC0", tmc_value & 0xFE);

                if self.select_vcan_mode {
                    // Send data to socket can
                    let socket = self.connect()?;
                    socket.write_frame(&socketcan::CANFrame::new(to_id, &data_slice[..data_len as usize], false, false).map_err(Error::SocketCanFrame)?)
                        .map_err(Error::SocketCanTransport)?;
                } else {
                    info!("Firmware sending out CAN data: {:?} at id {}", &data_slice[..data_len as usize], to_id); 
                }
            }
        } else if output == self.get_reg_addr("RFPCTR0").unwrap(){
            // When writting 0xFF to RFPCTR0 dequeue msg
            self.dequeue_can_msg().unwrap();
            info!("Writing to RFPCTR0, dequeueing message, msg_left in queue: {}", self.data_queue.len());
            
        } else {
            warn!("writting to RSCAN address {} have not been implemented yet", output);
        }

        // let socket = self.connect()?;
        // socket.write_frame(&socketcan::CANFrame::new(0xface, value, false, false).map_err(Error::SocketCanFrame)?)
        //     .map_err(Error::SocketCanTransport)?;
        Ok(())
    }

}