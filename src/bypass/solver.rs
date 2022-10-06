use fugue_concolic_solver_boolector::SolverContext;
use fugue_concolic::backend::ValueSolver;
use std::marker::PhantomData;
use std::collections::HashMap;
use std::fmt;
use log;

use fugue_concolic::expr::{
    SymExpr, IVar,
};
// use fugue_concolic::value::Value;
use metaemu::state::{
    pcode::PCodeState, };
use fugue::bytes::{Order};

use fugue::ir::{
    Address,
    il::pcode::{Operand, PCodeOp}
};

#[derive(Clone, Debug)]
struct Variables {
    name: String,       // The name of the variable
    version: u64,       // For each write operation, update the version number
    sym_expr: Vec<SymExpr>,    // The symbolic value hold by the variable
}

impl Variables {
    pub fn new(operand: &Operand, sym_expr: SymExpr) -> Self {
        // Either update the variable in the var_list or insert a new one
        if let Operand::Constant { value: _, size: _ } = *operand {
            panic!("Constant operand ({:#?}) should not be instered to variable list", operand);
        }
        let init_sym_expr_vec = vec![sym_expr];
        Self {
            name: Variables::gen_variable_name(operand).unwrap(),
            version: 0,
            sym_expr: init_sym_expr_vec,
        }
    }

    pub fn update(&mut self, sym_expr: SymExpr) {
        self.version += 1;
        self.sym_expr.push(sym_expr);
        // self.sym_expr = sym_expr;
    }

    // When the variable is rewritten, update its version number and name
    pub fn get_sym_var_name_next(&self) -> String{
        let version_next = self.version + 1;
        self.name.clone() + "-" + &version_next.to_string().clone()
    }

    // Generate operand name for Address, Regisiter, Variable
    // This can be used as key in the self.var_list to keep track of the value
    // And as the name of variables in Exprbuilder
    fn gen_variable_name(operand: &Operand) -> Option<String>{
        match operand {
            Operand::Address { value, size: _ } => {
                // Use the value as key for address
                Some(format!("{}", value))
            },
            Operand::Register { name, offset: _, size: _ } => {
                // Use its name as key for registers
                // TODO: Is offset needed to be added to the name?
                Some(name.to_string())
            },
            Operand::Variable { space, offset, size: _ } => {
                // Use "space:offset" as key for variables
                Some(format!("{:?}:{}", space, offset))
            }
            _ => {
                // No need to keep track of constant
                None
            }
        }
    }

    pub fn get_sym_expr(&self) -> &SymExpr {
        &self.sym_expr.last().unwrap()
    }
}
type StateValueType = u8;

#[derive(Clone)]
pub struct ConstraintSolver <O: Order> {
    // pm : PathManager,
    default_variables : HashMap<String, u128>,      // <Name of the default variable>: <value of the default variable>
    var_list: HashMap<String, Variables>,           // To keep track of regisiters and variables
    var_to_solve: HashMap<String, (SymExpr, Address)>,    // The variable to be solved, added when load happens
                                                                        //<Name of the variable>:(Symbex::variable, Address of the regisiter)
    // exp_to_solve: Vec<muexe_symbex::SymExpr>,    // The expression to solve
    order: PhantomData<O>,
}

impl<O: Order> fmt::Debug for ConstraintSolver<O> {
    // TODO: Add debug message for builder
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>)
    -> std::result::Result<(), std::fmt::Error> {
        f.debug_struct("ConstraintSolver")
        //  .field("pm", &self.pm)
         .field("builder", &String::from("TODO"))
         .finish()
    }
}

impl <O> ConstraintSolver <O>
where O: Order
{
    pub fn new() -> Self {
        Self{
            // pm: PathManager::new(),
            default_variables: HashMap::new(),
            var_list: HashMap::new(),
            var_to_solve: HashMap::new(),
            order: PhantomData,
            // exp_to_solve: Vec::new(),
        }
    }

    // pub fn add_default_variable(&mut self, var_name: String, value: u128) -> Result<(), &str>{
    //     self.default_variables.insert(var_name, value);
    //     Ok(())
    // }

    pub fn set_default_variables(&mut self, vars: &HashMap<String, u128>){
        self.default_variables = vars.clone();
    }

    fn var_list_insert(&mut self, operand: &Operand, expr: Option<SymExpr>) -> SymExpr{
        // Either update the variable in the var_list or insert a new one
        // return the latest value of the variable after insersion
        if let Operand::Constant { value: _, size: _ } = *operand {
            panic!("Constant operand ({:#?}) should not be instered to variable list", operand);
        }

        let key = Variables::gen_variable_name(&operand).unwrap();
        if self.var_list.contains_key(&key) {
            // If the variable exist,
            // then create a new symbel variable to represent its new state
            let var_in_list = self.var_list.get_mut(&key).unwrap();
            let updated_name = var_in_list.get_sym_var_name_next();

            // If the value of the variable is specified, then use it
            let var_updated = expr.unwrap_or(
                // let var_ecode = ;
                // If the value of the variable is not specified, then create new one
                SymExpr::ivar(IVar::new_named(&updated_name,
                    operand.size() as u32*8))
            );
            // And update its value in var_list
            var_in_list.update(var_updated.clone());
            var_updated

        } else{
            // If the variable doesn't exist, create new variable and insert it directly to var_list
            let key = Variables::gen_variable_name(operand).unwrap();
            // If the value of the variable is specified, then use it
            // If not, create a new variable
            let var = expr.unwrap_or(SymExpr::ivar(IVar::new_named(&key.clone(), operand.size() as u32*8)));
            // Add it to var_list
            self.var_list.insert(key, Variables::new(operand, var.clone()));
            var
        }
    }


    fn var_list_get(&mut self, state: &PCodeState<StateValueType, O>, operand: &Operand) -> &SymExpr {
        let var_name =Variables::gen_variable_name(operand).unwrap();

        // Search the variable list for it
        if self.var_list.contains_key(&var_name) {
            // If found in the variable list which has been previously written to
            // Then return the symbolic expression of it
            return self.var_list.get(&var_name).unwrap().get_sym_expr()
        }

        let default_value_some = self.default_variables.get(&var_name).clone();
        if let Some(default_value)  = default_value_some {
            // Check if it is in the default variable list
            // Create a constant based on its default value and set it to the var
            log::trace!("Variable({:?}) found in the default variable list", operand);
            if let Operand::Constant { value: _, size: _ } = operand {
                panic!("Operand {:?} is not a variable, but a constant", operand);
            } else {
                let operand_size_bits = operand.size() as u32 * 8;
                let var_value = SymExpr::val_sized(*default_value as u64, operand_size_bits.try_into().unwrap());
                self.var_list_insert(operand, Some(var_value));
                return self.var_list.get(&var_name).unwrap().get_sym_expr();
            }
        }else {
            // If not in the default variable list and not in the variable list and it's a regisiter
            // Then get the concrete value from the regisiter
            if let Operand::Register { name: _, offset: _, size } = operand {
                let reg_symbex = match size {
                    1 => {SymExpr::val(state.get_operand::<StateValueType>(operand).unwrap() as u8)}
                    2 => {SymExpr::val(state.get_operand::<StateValueType>(operand).unwrap() as u16)}
                    4 => {SymExpr::val(state.get_operand::<StateValueType>(operand).unwrap() as u32)}
                    8 => {SymExpr::val(state.get_operand::<StateValueType>(operand).unwrap() as u64)}
                    _ => {panic!("Unexpected size: {:?}", size);}
                };
                self.var_list_insert(operand, Some(reg_symbex));
                return self.var_list.get(&var_name).unwrap().get_sym_expr();
            }

            panic!("Variable({:?}), name({:?}) not in the list, default_vars: {:?}", operand, var_name, self.default_variables);
        }


    }

    // Generate corresponding SymExpr for reading Operand operations
    fn symexpr_from_operand_read(&mut self, state: &PCodeState<StateValueType, O>, operand: &Operand) -> SymExpr {
        match operand {
            Operand::Constant { value, size } => {
                // byte size to bit size
                SymExpr::val_sized(*value, *size * 8)     // TODO: Check if we can use BitVec in the Operand here
            }
            _ => {
                self.var_list_get(state, operand).clone()
            }
        }
    }



    pub fn add_pcode(&mut self, instruction: PCodeOp, state: &PCodeState<StateValueType, O>){
        match instruction.clone(){
            // Move
            PCodeOp::Load{source, destination, space: _} => {
                // When loading a variable from target memory, create a new variable to solve

                // The source will always be address
                let source_address = state.get_address(&source).unwrap(); // Read the real address
                let var_src = self.var_list_insert(&source, None);

                log::trace!("source_address: {}", source_address);

                // Mark it as a target variable to be solved
                self.var_to_solve.insert(Variables::gen_variable_name(&source).unwrap(), (var_src.clone(), source_address));

                // slice the source variable to match the destination size then insert
                let dest = var_src.extract_low(destination.size() as u32 * 8);

                // Creat dest and load src into it
                self.var_list_insert(&destination, Some(dest));

                log::trace!("Load: {:?} <- {:?}", destination, source);
            },
            PCodeOp::Copy{source, destination} => {
                let src_sym = self.symexpr_from_operand_read(state, &source);
                self.var_list_insert(&destination, Some(src_sym));
            },

            // Branch
            PCodeOp::CBranch { destination: _, condition: _ } =>{
                // The final expression to be solved
                // let op_sym = self.symexpr_from_operand_read(&condition);
                // self.exp_to_solve.push(op_sym.clone());

            },
            PCodeOp::Return { destination: _ } =>{
                // No effect for building the tree
            },
            PCodeOp::Call { destination: _} => {
               // No effect for building the tree
            },
            PCodeOp::Branch { destination: _ } => {
               // No effect for building the tree
            },
            PCodeOp::Store { source, destination, space: _ } => {
                let src_sym = self.symexpr_from_operand_read(state, &source);
                self.var_list_insert(&destination, Some(src_sym));
            },
            ////////////////////////////////////////////////
            // Bitwise Operations
            PCodeOp::IntAnd{result, operands} => {
                // get the operands
                let op1 = &operands[0];
                let op2 = &operands[1];

                let op1_sym = self.symexpr_from_operand_read(state, op1);
                let op2_sym = self.symexpr_from_operand_read(state, op2);

                let result_sym = SymExpr::and(op1_sym, op2_sym);
                // insert result
                self.var_list_insert(&result, Some(result_sym));

            },
            PCodeOp::BoolAnd { result, operands } => {
                let op1_sym = self.symexpr_from_operand_read(state, &operands[0]);
                let op2_sym = self.symexpr_from_operand_read(state, &operands[1]);
                let result_sym = SymExpr::bool_and(op1_sym, op2_sym);

                // insert result
                self.var_list_insert(&result, Some(result_sym));

            },
            PCodeOp::IntOr { result, operands } => {
                let op1_sym = self.symexpr_from_operand_read(state, &operands[0]);
                let op2_sym = self.symexpr_from_operand_read(state, &operands[1]);

                let result_sym = SymExpr::or(op1_sym, op2_sym);

                // insert result
                self.var_list_insert(&result, Some(result_sym));

            },
            PCodeOp::BoolXor { result, operands } => {
                let op1_sym = self.symexpr_from_operand_read(state, &operands[0]);
                let op2_sym = self.symexpr_from_operand_read(state, &operands[1]);

                let result_sym = SymExpr::bool_xor(op1_sym, op2_sym);

                // insert result
                self.var_list_insert(&result, Some(result_sym));
            },
            PCodeOp::IntXor { result, operands } => {
                let op1_sym = self.symexpr_from_operand_read(state, &operands[0]);
                let op2_sym = self.symexpr_from_operand_read(state, &operands[1]);

                let result_sym = SymExpr::xor(op1_sym, op2_sym);

                // insert result
                self.var_list_insert(&result, Some(result_sym));
            },
            PCodeOp::IntLeftShift { result, operands } => {
                let op1_sym = self.symexpr_from_operand_read(state, &operands[0]);
                let op2_sym = self.symexpr_from_operand_read(state, &operands[1]);
                let result_sym = SymExpr::shl(op1_sym, op2_sym);

                // insert result
                self.var_list_insert(&result, Some(result_sym));
            },
            PCodeOp::IntRightShift { result, operands } => {
                let op1_sym = self.symexpr_from_operand_read(state, &operands[0]);
                let op2_sym = self.symexpr_from_operand_read(state, &operands[1]);
                let result_sym = SymExpr::shr(op1_sym, op2_sym);

                // insert result
                self.var_list_insert(&result, Some(result_sym));
            },
            PCodeOp::IntSRightShift { result, operands } => {
                let op1_sym = self.symexpr_from_operand_read(state, &operands[0]);
                let op2_sym = self.symexpr_from_operand_read(state, &operands[1]);

                let result_sym = SymExpr::signed_shr(op1_sym, op2_sym);

                // insert result
                self.var_list_insert(&result, Some(result_sym));
            },
            PCodeOp::IntNot { result, operand } => {
                let op1_sym = self.symexpr_from_operand_read(state, &operand);
                let result_sym = SymExpr::not(op1_sym);
                // insert result
                self.var_list_insert(&result, Some(result_sym));
            },
            ////////////////////////////////////////////////
            // Change size
            PCodeOp::IntZExt { result, operand } => {
                let op_sym = self.symexpr_from_operand_read(state, &operand);
                let result_sym = SymExpr::zero_extend(op_sym, result.size() as u32*8); // Byte count to bit count
                self.var_list_insert(&result, Some(result_sym));
            },
            PCodeOp::IntSExt { result, operand } => {
                let op_sym = self.symexpr_from_operand_read(state, &operand);
                let result_sym = SymExpr::sign_extend(op_sym, result.size() as u32*8); // Byte count to bit count
                self.var_list_insert(&result, Some(result_sym));
            },
            PCodeOp::Subpiece { result, operand, amount } => {
                let op_sym = self.symexpr_from_operand_read(state, &operand);
                // Parse the *amount* argument and extract bits in *operands* according to it
                if let Operand::Constant { value, size: _ } = amount {
                    // Fill up to the size of the output,
                    // so get the smaller size between the amount and the result
                    let bits_perserve = (operand.size() as u64 - value) * 8;    // Convert bytes to throw away to bits to perserve
                    let bits_result = result.size() as u64 *8;
                    let bits_smaller = std::cmp::min(bits_perserve, bits_result);
                    // TODO: use LE/BE to determie which side to start counting
                    let result_sym = SymExpr::extract(op_sym, 0, bits_smaller as u32);
                    self.var_list_insert(&result, Some(result_sym));
                } else {
                    panic!("Should not happen: Subpiece: amount is not a constant {:?}", instruction);
                }
            },
            ////////////////////////////////////////////////
            // Logical Operation
            PCodeOp::IntEq{result, operands} => {
                let op1_sym = self.symexpr_from_operand_read(state, &operands[0]);
                let op2_sym = self.symexpr_from_operand_read(state, &operands[1]);

                let result_sym = SymExpr::eq(op1_sym, op2_sym);

                // insert result
                self.var_list_insert(&result, Some(result_sym));
            },
            PCodeOp::IntNotEq { result, operands } => {
                let op1_sym = self.symexpr_from_operand_read(state, &operands[0]);
                let op2_sym = self.symexpr_from_operand_read(state, &operands[1]);

                let result_sym = SymExpr::ne(op1_sym, op2_sym);

                // insert result
                self.var_list_insert(&result, Some(result_sym));
            },
            PCodeOp::IntSLess { result, operands } => {
                let op1_sym = self.symexpr_from_operand_read(state, &operands[0]);
                let op2_sym = self.symexpr_from_operand_read(state, &operands[1]);

                let result_sym = SymExpr::slt(op1_sym, op2_sym);

                // insert result
                self.var_list_insert(&result, Some(result_sym));

            },
            PCodeOp::IntLess { result, operands } => {
                let op1_sym = self.symexpr_from_operand_read(state, &operands[0]);
                let op2_sym = self.symexpr_from_operand_read(state, &operands[1]);

                let result_sym = SymExpr::lt(op1_sym, op2_sym);

                // insert result
                self.var_list_insert(&result, Some(result_sym));

            },
            ////////////////////////////////////////////////
            // Arithmetic
            PCodeOp::IntSub { result, operands } => {
                let op1_sym = self.symexpr_from_operand_read(state, &operands[0]);
                let op2_sym = self.symexpr_from_operand_read(state, &operands[1]);

                // works for both signed and unsigned
                let result_sym = SymExpr::sub(op1_sym, op2_sym);

                // insert result
                self.var_list_insert(&result, Some(result_sym));

            },
            PCodeOp::IntAdd { result, operands } => {
                let op1_sym = self.symexpr_from_operand_read(state, &operands[0]);
                let op2_sym = self.symexpr_from_operand_read(state, &operands[1]);
                // Works for both signed and unsigned
                let result_sym = SymExpr::add(op1_sym, op2_sym);

                // insert result
                self.var_list_insert(&result, Some(result_sym));
            },
            PCodeOp::IntNeg { result, operand } => {
                let op1_sym = self.symexpr_from_operand_read(state, &operand);
                let result_sym = SymExpr::neg(op1_sym);

                self.var_list_insert(&result, Some(result_sym));
            },
            PCodeOp::IntCarry { result, operands } =>{
                let op1_sym = self.symexpr_from_operand_read(state, &operands[0]);
                let op2_sym = self.symexpr_from_operand_read(state, &operands[1]);
                // Works for both signed and unsigned
                let result_sym = SymExpr::carry(op1_sym, op2_sym);

                // insert result
                self.var_list_insert(&result, Some(result_sym));
            },
            PCodeOp::IntSCarry { result, operands } =>{
                let op1_sym = self.symexpr_from_operand_read(state, &operands[0]);
                let op2_sym = self.symexpr_from_operand_read(state, &operands[1]);
                // Works for both signed and unsigned
                let result_sym = SymExpr::signed_carry(op1_sym, op2_sym);

                // insert result
                self.var_list_insert(&result, Some(result_sym));
            },
            PCodeOp::BoolOr { result, operands } => {
                let op1_sym = self.symexpr_from_operand_read(state, &operands[0]);
                let op2_sym = self.symexpr_from_operand_read(state, &operands[1]);
                // Works for both signed and unsigned
                let result_sym = SymExpr::or(op1_sym, op2_sym);
                // insert result
                self.var_list_insert(&result, Some(result_sym));
            }
            PCodeOp::Skip => (),

            _ => {
                let pc = state.program_counter_value().unwrap();
                panic!("PC {} Instruction({:?}) not yet supported by solver.", pc, instruction);
            }
        }
    }


    pub fn solve(&mut self, state: &PCodeState<StateValueType, O>, operand: &Operand, expected_value: u64) -> Option<HashMap::<Address, Option<u64>>>{
        // Solve an expression as specified by operand
        // operand: the operand to be solved

        // Get the symexpr of the operand
        let op_sym = self.symexpr_from_operand_read(state, operand);

        let size_op_sym =op_sym.bits(); 

        // Generate the expected value
        let expected_sym = SymExpr::val_sized(expected_value, size_op_sym as usize);

        // Build AST for the final expression
        let mut solver_context = SolverContext::new_independent();
        
        // Add constraint that the expected value is equal to the operand
        let constraint = op_sym.eq(expected_sym);
        // let solve_result = op_sym.solve(&mut solver_context, &[constraint]);

        // Solve the vars in var_to_solve list

        // The solving results to be returned: a list of (address, value)
        let mut return_res = HashMap::<Address, Option<u64>>::new();

        for (expr, addr) in self.var_to_solve.values(){
            log::debug!("Solving target: {}", expr);
            log::debug!("Solving constraint: {}", constraint);
            let solve_res = expr.solve(&mut solver_context, &[constraint.clone()]);
            match solve_res{
                Some(val) => {
                    let val_u64 = val.to_u64();
                    log::debug!("Solver: solution found for address {}", addr);
                    return_res.insert(*addr, val_u64);
                },
                None => {
                    // solution not found for the variable
                    log::warn!("Solver: No solution found for address {}", addr);
                    return_res.insert(*addr, None);
                }
            }
        }

        if return_res.len() == 0{
            return None;
        } else {
            return Some(return_res);
        }
    }
}
