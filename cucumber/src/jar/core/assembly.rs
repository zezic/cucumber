use krakatau2::lib::{
    assemble,
    classfile::parse::Class,
    classfile::{
        attrs::{AttrBody, Attribute},
        cpool::ConstPool,
    },
    disassemble::refprinter::RefPrinter,
    AssemblerOptions, DisassemblerOptions,
};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum ReasmError {
    #[error("Assemble error: {0:?}")]
    Assemble(krakatau2::lib::AssembleError),
    #[error("Disassemble error: {0}")]
    Disassemble(std::io::Error),
    #[error("Source parse error: {0}")]
    SourceParse(#[from] std::str::Utf8Error),
}

/// Reassemble a class by disassembling to source and then assembling back to bytecode
pub fn reasm(class: &Class<'_>) -> Result<Vec<u8>, ReasmError> {
    let mut out = Vec::new();

    krakatau2::lib::disassemble::disassemble(
        &mut out,
        &class,
        DisassemblerOptions { roundtrip: true },
    )
    .map_err(ReasmError::Disassemble)?;

    let source = std::str::from_utf8(&out)?;
    let mut assembled = assemble(source, AssemblerOptions {}).map_err(ReasmError::Assemble)?;
    let (_name, data) = assembled.pop().unwrap();

    Ok(data)
}

/// Initialize a RefPrinter for a class's constant pool and attributes
pub fn init_refprinter<'a>(cp: &ConstPool<'a>, attrs: &'a [Attribute<'a>]) -> RefPrinter<'a> {
    let mut bstable = None;
    let mut inner_classes = None;
    for attr in attrs {
        use AttrBody::*;
        match &attr.body {
            BootstrapMethods(v) => bstable = Some(v.as_ref()),
            InnerClasses(v) => inner_classes = Some(v.as_ref()),
            _ => {}
        }
    }

    let rp = RefPrinter::new(true, &cp, bstable, inner_classes);

    rp
}
