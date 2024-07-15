
use cairo_lang_sierra::{
    ids::{ConcreteLibfuncId, ConcreteTypeId, GenericLibfuncId, VarId},
    program::{
        ConcreteLibfuncLongId, GenBranchInfo,
        GenBranchTarget, GenInvocation, GenStatement, GenericArg, LibfuncDeclaration,
        StatementIdx,
    },
};
use inkwell::values::{AnyValue, BasicValueEnum};
use inkwell::{values::InstructionValue};
use num_bigint::BigInt;
use smol_str::SmolStr;

use crate::SierraBuilder;

impl<'ctx> SierraBuilder<'ctx> {
    pub fn push_simple_basic_statement(
        &mut self,
        libfunc_id: ConcreteLibfuncId,
        args: &[cairo_lang_sierra::ids::VarId],
        results: &[cairo_lang_sierra::ids::VarId],
    ) {
        let statement = GenStatement::Invocation(GenInvocation {
            libfunc_id,
            args: args.into(),
            branches: vec![GenBranchInfo {
                target: GenBranchTarget::Fallthrough,
                results: results.into(),
            }],
        });
        self.program.statements.push(statement);
    }

    pub fn push_store_temp_statement(
        &mut self,
        libfunc_id: ConcreteLibfuncId,
        ty: String,
        args: &[cairo_lang_sierra::ids::VarId],
        results: &[cairo_lang_sierra::ids::VarId],
    ) {
        let func = LibfuncDeclaration {
            id: ConcreteLibfuncId::from_string("store_temp"),
            long_id: ConcreteLibfuncLongId {
                generic_id: GenericLibfuncId::from_string("store_temp"),
                generic_args: vec![GenericArg::Type(ConcreteTypeId::from_string(ty.clone()))],
            },
        };
        if self.libfuncs.insert("store_temp".to_owned()) {
            self.program.libfunc_declarations.push(func);
        }
        let statement = GenStatement::Invocation(GenInvocation {
            libfunc_id,
            args: args.into(),
            branches: vec![GenBranchInfo {
                target: GenBranchTarget::Fallthrough,
                results: results.into(),
            }],
        });
        self.program.statements.push(statement);
    }

    pub fn build_jump_basic_statement(
        &self,
        libfunc_id: ConcreteLibfuncId,
        dest1: u32,
        dest2: u32,
    ) -> GenStatement<StatementIdx> {
        GenStatement::Invocation(GenInvocation {
            libfunc_id,
            args: Vec::new(),
            branches: vec![
                GenBranchInfo {
                    target: GenBranchTarget::Statement(StatementIdx(dest1 as usize)),
                    results: Vec::new(),
                },
                GenBranchInfo {
                    target: GenBranchTarget::Statement(StatementIdx(dest2 as usize)),
                    results: Vec::new(),
                },
            ],
        })
    }
    pub fn build_binary_int_func(
        &mut self,
        instr: InstructionValue<'ctx>,
        concrete_id: ConcreteLibfuncId,
    ) {
        // Get the 2 operands ex: in `let _ = a == b;` we get a and b
        let first_val = unsafe { instr.get_operand_unchecked(0).unwrap().left().unwrap() };
        let scnd_val = unsafe { instr.get_operand_unchecked(1).unwrap().left().unwrap() };
        // get their types
        let mut first_ty = first_val.get_type().to_string();
        let mut scnd_ty = scnd_val.get_type().to_string();
        // removes the quotes
        first_ty.retain(|c| c != '"');
        scnd_ty.retain(|c| c != '"');

        // sanity check
        assert_eq!(first_ty, scnd_ty, "Comparison should have the same types");
        // Insert the types in sierra program (only need one as they're equal)
        self.insert_type(first_ty.clone());
        // Get the condition
        // Create the const function if one of the operands is a const. Add it to the declaration and statements
        self.add_const_if_const(first_val, first_ty.clone());
        self.add_const_if_const(scnd_val, scnd_ty.clone());
        // Args of the comparison function
        let args = [
            self.variables.get(&first_val).unwrap().clone(),
            self.variables.get(&scnd_val).unwrap().clone(),
        ];
        // result variable of the comparison
        let mut result_var_id = VarId {
            id: self.next_var() as u64,
            debug_name: None,
        };
        if let Ok(basic_value_enum) = instr.as_any_value_enum().try_into() {
            self.variables
                .insert(basic_value_enum, result_var_id.clone());
            let res_name = basic_value_enum.get_name().to_str().unwrap();
            result_var_id.debug_name = (!res_name.is_empty()).then_some(SmolStr::from(res_name));
        }
        // Insert the function call in the statements and declaration
        self.push_simple_basic_statement(concrete_id, &args, &[result_var_id]);
    }

    /// Adds a const function if the int value is a const. Adds the libfunc declaration and adds the call in the
    /// statements list as well.
    pub fn add_const_if_const(&mut self, val: BasicValueEnum<'ctx>, ty: String) {
        let val_int = val.into_int_value();
        if val_int.is_constant_int() {
            // Get the llvm value of the const so smth like `i32 0` if it's a const
            let int_value = val_int
                .print_to_string()
                .to_string()
                .split_whitespace()
                .last()
                .unwrap()
                .to_owned();

            let fn_name = format!("const_as_immediate<{}, {}>", ty, int_value);

            let func = LibfuncDeclaration {
                id: ConcreteLibfuncId::from_string(&fn_name),
                long_id: ConcreteLibfuncLongId {
                    generic_id: GenericLibfuncId::from_string("const"),
                    generic_args: vec![
                        GenericArg::Type(ConcreteTypeId::from_string(ty.clone())),
                        GenericArg::Value(BigInt::from(int_value.parse::<i128>().unwrap())),
                    ],
                },
            };
            // if not declared yet declare it
            if self.libfuncs.insert(fn_name.clone()) {
                self.program.libfunc_declarations.push(func);
            }
            // Var id for the const.
            let next_var = VarId {
                id: self.next_var() as u64,
                debug_name: Some(SmolStr::from(format!("const_{}<{}>", ty, int_value))),
            };
            // Add the const call to the statement.
            self.push_simple_basic_statement(
                ConcreteLibfuncId::from_string(fn_name),
                &[],
                &[next_var.clone()],
            );

            self.variables.insert(val, next_var);
        }
    }
}
