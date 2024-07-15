use std::{
    collections::{HashMap, HashSet},
    path::Path,
};

use cairo_lang_sierra::{
    ids::{ConcreteLibfuncId, ConcreteTypeId, GenericLibfuncId, GenericTypeId, VarId},
    program::{
        ConcreteLibfuncLongId, ConcreteTypeLongId, DeclaredTypeInfo, GenStatement,
        LibfuncDeclaration, Program, StatementIdx, TypeDeclaration,
    },
};
use inkwell::memory_buffer::MemoryBuffer;
use inkwell::values::{AnyValue, AsValueRef, BasicValueEnum, InstructionOpcode};
use inkwell::{basic_block::BasicBlock, context::Context, values::PhiValue};
use smol_str::SmolStr;
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Label(u32);

struct SierraBuilder<'ctx> {
    libfuncs: HashSet<String>,
    funcs: HashSet<(String, InstructionOpcode)>,
    types: HashSet<String>,
    program: Program,
    variables: HashMap<BasicValueEnum<'ctx>, VarId>,
    block_remapping: HashMap<BasicBlock<'ctx>, StatementIdx>,
    jumps: HashMap<String, (BasicBlock<'ctx>, BasicBlock<'ctx>)>,
    jump_to_phi: HashMap<BasicBlock<'ctx>, HashSet<(VarId, String, BasicValueEnum<'ctx>)>>,
    next_var: u32,
}
pub mod utils;

impl<'ctx> Default for SierraBuilder<'ctx> {
    fn default() -> Self {
        Self {
            program: Program {
                type_declarations: Vec::default(),
                libfunc_declarations: Vec::default(),
                statements: Vec::default(),
                funcs: Vec::default(),
            },
            libfuncs: HashSet::default(),
            block_remapping: HashMap::default(),
            funcs: HashSet::default(),
            types: HashSet::default(),
            variables: HashMap::default(),
            jumps: HashMap::default(),
            jump_to_phi: HashMap::default(),
            next_var: u32::default(),
        }
    }
}

impl<'ctx> SierraBuilder<'ctx> {
    pub fn next_var(&mut self) -> u32 {
        let val = self.next_var;
        self.next_var += 1;
        val
    }
    /// Insert type in type declaration if needed
    pub fn insert_type(&mut self, mut ty: String) {
        ty.retain(|c| c != '"');
        if self.types.insert(ty.clone()) {
            self.program.type_declarations.push(TypeDeclaration {
                id: ConcreteTypeId::from_string(ty.clone()),
                long_id: ConcreteTypeLongId {
                    generic_id: GenericTypeId::from_string(ty.clone()),
                    generic_args: vec![],
                },
                declared_type_info: Some(DeclaredTypeInfo {
                    storable: true,
                    droppable: true,
                    duplicatable: true,
                    zero_sized: false,
                }),
            })
        }
    }

    /// Insert function parameters (insert type + creates sierra variables)
    pub fn insert_param(&mut self, param: BasicValueEnum<'ctx>) {
        self.insert_type(param.get_type().to_string());
        let next_var = self.next_var();
        self.variables.insert(
            param,
            VarId {
                id: next_var as u64,
                debug_name: Some(SmolStr::from(param.get_name().to_str().unwrap())),
            },
        );
    }

    /// Read an llvm file and generate fully unfunctionnal sierra.
    pub fn compile() {
        // Initialize LLVM context
        let context = Context::create();

        let mut builder = SierraBuilder::default();
        // Parse the LLVM IR
        let module = context
            .create_module_from_ir(
                MemoryBuffer::create_from_file(Path::new("fib.ll"))
                    .expect("Failed to load llvm file"),
            )
            .expect("Failed to parse LLVM IR");

        // Collect all the basic blocks where a jump leads to a phi instruction to store the value in a tempvar before jumping
        // phi basically merges branches to allow let a = if cond { some_val } else { some_other_val}
        for function in module.get_functions() {
            let mut first_var_id = function.count_params();
            for basic_block in function.get_basic_block_iter() {
                for instr in basic_block.get_instructions() {
                    if let InstructionOpcode::Phi = instr.get_opcode() {
                        unsafe {
                            // Get the 2 basic blocks that contain the jump instruction that jump here
                            PhiValue::new(instr.as_value_ref())
                                .get_incomings()
                                .for_each(|inc| {
                                    // Append the set if it already exists (case where multiple jumps in the same BB land to this phi instruction)
                                    // else just create it
                                    let mut curr_set =
                                        if let Some(curr_set) = builder.jump_to_phi.get(&inc.1) {
                                            curr_set.clone()
                                        } else {
                                            HashSet::default()
                                        };

                                    // This var id correspond to the result var where we'll store the value before jumping
                                    let var_id = VarId {
                                        id: first_var_id as u64,
                                        debug_name: Some(SmolStr::from(
                                            inc.0.get_name().to_str().unwrap(),
                                        )),
                                    };
                                    curr_set.insert((
                                        var_id.clone(),
                                        instr.get_type().print_to_string().to_string(),
                                        inc.0,
                                    ));
                                    if let Ok(basic_value_enum) =
                                        instr.as_any_value_enum().try_into()
                                    {
                                        builder.variables.insert(basic_value_enum, var_id);
                                    }
                                    builder.jump_to_phi.insert(inc.1, curr_set);
                                    first_var_id += 1;
                                })
                        }
                    };
                }
            }

            builder.next_var = first_var_id;
        }

        // Iterate over functions and basic blocks
        for function in module.get_functions() {
            function.get_param_iter().for_each(|param| {
                builder.insert_param(param);
            });

            for basic_block in function.get_basic_blocks() {
                builder
                    .block_remapping
                    .insert(basic_block, StatementIdx(builder.program.statements.len()));
                for instr in basic_block.get_instructions() {
                    match instr.get_opcode() {
                        InstructionOpcode::ICmp => {
                            // Get the comparison op
                            let cond = match instr.get_icmp_predicate().unwrap() {
                                inkwell::IntPredicate::EQ => "eq",
                                _ => "baboum",
                            };
                            // get the type of the operands
                            let ty = instr
                                .get_operand(0)
                                .unwrap()
                                .left()
                                .unwrap()
                                .get_type()
                                .print_to_string()
                                .to_string();
                            // Format sierra libfunc name
                            let name = format!("{}_{}", &ty, cond);
                            // get a concrete function id
                            let concrete_id = ConcreteLibfuncId::from_string(name.clone());
                            builder.build_binary_int_func(instr, concrete_id.clone());
                            if builder.funcs.insert((ty, InstructionOpcode::ICmp)) {
                                builder
                                    .program
                                    .libfunc_declarations
                                    .push(LibfuncDeclaration {
                                        id: concrete_id.clone(),
                                        long_id: ConcreteLibfuncLongId {
                                            generic_id: GenericLibfuncId::from_string(name),
                                            generic_args: vec![],
                                        },
                                    });
                            }
                        }
                        InstructionOpcode::Add => {
                            // Format sierra libfunc name
                            let name =
                                format!("{}_add", &instr.get_type().print_to_string().to_string());
                            // get a concrete function id
                            let concrete_id = ConcreteLibfuncId::from_string(name.clone());
                            builder.build_binary_int_func(instr, concrete_id);
                        }
                        InstructionOpcode::Br => {
                            let fn_id = ConcreteLibfuncId::from_string("jump");
                            // Get the phis from the mapping we created earlier
                            let phis = if let Some(var_ids) = builder.jump_to_phi.get(&basic_block)
                            {
                                var_ids.clone()
                            } else {
                                HashSet::default()
                            }
                            .clone();
                            // If there is a jump in this basic block that leads to a phi we'll store the value it has to merge in a temp var
                            // Highly unoptimized
                            phis.iter().for_each(|(var_id, ty, var)| {
                                builder.push_store_temp_statement(
                                    ConcreteLibfuncId::from_string("store_temp"),
                                    ty.clone(),
                                    &[builder
                                        .variables
                                        .get(var)
                                        .expect("Target value should be set before the jump")
                                        .clone()],
                                    &[var_id.clone()],
                                )
                            });
                            let func = LibfuncDeclaration {
                                id: ConcreteLibfuncId::from_string("jump"),
                                long_id: ConcreteLibfuncLongId {
                                    generic_id: GenericLibfuncId::from_string("jump"),
                                    generic_args: vec![],
                                },
                            };
                            if builder.libfuncs.insert(func.to_string()) {
                                builder.program.libfunc_declarations.push(func);
                            }
                            let statement =
                                builder.build_jump_basic_statement(fn_id, u32::MAX, u32::MAX);
                            builder.program.statements.push(statement.clone());
                            unsafe {
                                builder.jumps.insert(
                                    statement.to_string(),
                                    (
                                        instr.get_operand_unchecked(1).unwrap().right().unwrap(),
                                        instr.get_operand_unchecked(2).unwrap().right().unwrap(),
                                    ),
                                )
                            };
                        }
                        InstructionOpcode::Return => {
                            //
                            builder.program.statements.push(GenStatement::Return(
                                instr
                                    .get_operands()
                                    .map(|op| {
                                        builder
                                            .variables
                                            .get(&op.unwrap().left().unwrap())
                                            .unwrap()
                                            .clone()
                                    })
                                    .collect::<Vec<_>>(),
                            ));
                        }
                        _ => (),
                    }
                }
            }
        }
        builder.program.statements = builder
            .program
            .statements
            .iter()
            .map(|statement| {
                if statement.to_string().contains("jump") {
                    let (false_block, true_block) =
                        builder.jumps.get(&statement.to_string()).unwrap();
                    let dest1 = builder.block_remapping.get(false_block).unwrap();
                    let dest2 = builder.block_remapping.get(true_block).unwrap();
                    let fn_id = ConcreteLibfuncId::from_string("jump");
                    builder.build_jump_basic_statement(fn_id, dest1.0 as u32, dest2.0 as u32)
                } else {
                    statement.clone()
                }
            })
            .collect::<Vec<_>>();
        println!("{}", builder.program);
    }
}

fn main() {
    SierraBuilder::compile();
}
