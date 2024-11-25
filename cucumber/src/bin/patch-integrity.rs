use std::{env, fs::{self, File}, io::BufWriter, path::Path};
use std::io::Read;

use anyhow::anyhow;

use cucumber::{extract_general_goodies, types::{AbsoluteColor, ColorConst, CucumberBitwigTheme, NamedColor, UiTarget}};
use krakatau2::{file_output_util::Writer, lib::{assemble, classfile::{self, code::Instr, parse::Class}, AssemblerOptions, DisassemblerOptions, ParserOptions}, zip};

fn main() -> anyhow::Result<()> {
    let args: Vec<String> = env::args().collect();
    let input_jar = &args[1];
    let output_jar = &args[2];

    let file = fs::File::open(input_jar)?;
    let mut zip = zip::ZipArchive::new(file)?;

    let general_goodies = extract_general_goodies(&mut zip, |_| {})?;

    let mut file = zip.by_name(&general_goodies.init_class).unwrap();
    let mut buffer = Vec::new();
    file.read_to_end(&mut buffer)?;

    let mut class = classfile::parse(
        &buffer,
        ParserOptions {
            no_short_code_attr: true,
        },
    )
    .map_err(|err| anyhow!("Parse: {:?}", err))?;

    patch_class(&mut class);

    let patched = reasm(&class).unwrap();
    drop(file);

    let mut writer = Writer::new(Path::new(output_jar))?;

    let mut class_buf = Vec::new();
    for i in 0..zip.len() {
        let mut file = zip.by_index(i)?;
        let name = file.name().to_owned();

        if name.eq(&general_goodies.init_class) {
            writer.write(Some(&name), &patched)?;
        } else {
            class_buf.clear();
            class_buf.reserve(file.size() as usize);
            file.read_to_end(&mut class_buf)?;
            writer.write(Some(&name), &class_buf)?;
        }
    }

    Ok(())
}

fn patch_class(class: &mut Class<'_>) {
    for method in &mut class.methods {
        let Some(attr) = method.attrs.first_mut() else { continue; };
        let classfile::attrs::AttrBody::Code((code_1, _code_2)) = &mut attr.body else { continue; };
        let bytecode = &mut code_1.bytecode;
        let mut new_bytecode = vec![];
        for (pos, ix) in bytecode.0.drain(..) {
            new_bytecode.push((pos, ix));
            let len = new_bytecode.len();
            if len < 3 {
                continue;
            }
            let mut ixs = &mut new_bytecode[len - 3..];
            if ixs.len() != 3 {
                continue;
            }
            if let [(_, ix), (_, Instr::Sipush(5000)), (_, Instr::IfIcmple(_))] = &mut ixs {
                *ix = Instr::Sipush(0);
            }
        }
        bytecode.0 = new_bytecode;
    }
}

fn reasm(class: &Class<'_>) -> anyhow::Result<Vec<u8>> {
    let mut out = Vec::new();
    krakatau2::lib::disassemble::disassemble(
        &mut out,
        &class,
        DisassemblerOptions { roundtrip: true },
    )?;

    let source = std::str::from_utf8(&out)?;
    let mut assembled =
        assemble(source, AssemblerOptions {}).map_err(|err| anyhow!("Asm: {:?}", err))?;
    let (_name, data) = assembled.pop().unwrap();

    Ok(data)
}