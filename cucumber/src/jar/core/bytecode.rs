use krakatau2::lib::{
    classfile::code::Instr,
    disassemble::refprinter::{ConstData, RefPrinter},
};

/// Trait for converting bytecode instructions to integer values
pub trait IxToInt {
    fn to_int(&self) -> u8;
}

/// Trait for converting bytecode instructions to float values
pub trait IxToFloat {
    fn to_float(&self, refprinter: &RefPrinter) -> f32;
}

/// Trait for converting bytecode instructions to double values
pub trait IxToDouble {
    fn to_double(&self, refprinter: &RefPrinter) -> f64;
}

impl IxToInt for Instr {
    fn to_int(&self) -> u8 {
        match self {
            Instr::Iconst0 => 0,
            Instr::Iconst1 => 1,
            Instr::Iconst2 => 2,
            Instr::Iconst3 => 3,
            Instr::Iconst4 => 4,
            Instr::Iconst5 => 5,
            Instr::Lconst0 => 0,
            Instr::Lconst1 => 1,
            Instr::Bipush(x) => *x as u8,
            Instr::Sipush(x) => *x as u8,
            x => unimplemented!("instr: {:?}", x),
        }
    }
}

impl IxToFloat for Instr {
    fn to_float(&self, refprinter: &RefPrinter) -> f32 {
        let id = match self {
            Instr::Fconst0 => return 0.0,
            Instr::Fconst1 => return 1.0,
            Instr::Fconst2 => return 2.0,
            Instr::Dconst0 => return 0.0,
            Instr::Dconst1 => return 1.0,
            Instr::Ldc(ind) => *ind as u16,
            Instr::LdcW(ind) => *ind,
            x => unimplemented!("instr: {:?}", x),
        };
        let data = refprinter.cpool.get(id as usize).unwrap();
        match &data.data {
            ConstData::Prim(_prim_tag, text) => match text.trim_end_matches("f").parse::<f32>() {
                Ok(val) => val,
                Err(err) => {
                    panic!("err parse f32 [{}]: {}", text, err);
                }
            },
            _ => unimplemented!(),
        }
    }
}

impl IxToDouble for Instr {
    fn to_double(&self, refprinter: &RefPrinter) -> f64 {
        match self {
            Instr::Fconst0 => 0.0,
            Instr::Fconst1 => 1.0,
            Instr::Fconst2 => 2.0,
            Instr::Dconst0 => 0.0,
            Instr::Dconst1 => 1.0,
            Instr::Ldc2W(ind) => {
                let data = refprinter.cpool.get(*ind as usize).unwrap();
                match &data.data {
                    ConstData::Prim(_prim_tag, text) => {
                        match text.trim_end_matches("d").parse::<f64>() {
                            Ok(val) => val,
                            Err(err) => {
                                panic!("err parse f64 [{}]: {}", text, err);
                            }
                        }
                    }
                    _ => unimplemented!(),
                }
            }
            x => unimplemented!("instr: {:?}", x),
        }
    }
}
