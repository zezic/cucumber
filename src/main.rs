use std::{env, fs, io::Read, path::Path, time::Instant};

use anyhow::{anyhow, Result};

use indicatif::ProgressBar;
use krakatau2::{
    file_output_util::Writer,
    lib::{
        assemble,
        disassemble::refprinter::{self, RefOrString, ConstData, SingleTag, UtfData},
        classfile::{self, code::Instr, parse::Class, attrs::AttrBody},
        AssemblerOptions, DisassemblerOptions, ParserOptions,
    },
    zip,
};

fn main() -> Result<()> {
    let args: Vec<String> = env::args().collect();

    let jar_name = &args[1];
    let output_name = &args[2];

    let mut class_buf = Vec::new();
    let file = fs::File::open(jar_name)?;
    let mut zip = zip::ZipArchive::new(file)?;
    let class_ext = ".class";

    let now = Instant::now();

    let mut classes = vec![];

    let bar = ProgressBar::new(zip.len() as u64);

    for i in 0..zip.len() {
        bar.inc(1);
        let mut file = zip.by_index(i)?;

        let name = file.name().to_owned();
        if !name.trim_end_matches('/').ends_with(&class_ext) {
            continue;
        }

        class_buf.clear();
        class_buf.reserve(file.size() as usize);
        file.read_to_end(&mut class_buf)?;

        let patched = patch_data(&name, &class_buf)?;

        classes.push((name, patched));
    }

    bar.finish();

    let dur = Instant::now().duration_since(now);
    println!("Patched: {:?}", dur);

    let mut writer = Writer::new(Path::new(output_name))?;

    let now = Instant::now();

    for (name, data) in classes {
        writer.write(Some(&name), &data)?;
    }

    let dur = Instant::now().duration_since(now);
    println!("Writed: {:?}", dur);

    Ok(())
}

fn trace_class(class: &Class<'_>) {
    let mut bstable = None;
    let mut inner_classes = None;
    for attr in &class.attrs {
        use AttrBody::*;
        match &attr.body {
            BootstrapMethods(v) => bstable = Some(v.as_ref()),
            InnerClasses(v) => inner_classes = Some(v.as_ref()),
            _ => {}
        }
    }

    let rp = refprinter::RefPrinter::new(true, &class.cp, bstable, inner_classes);
    // dbg!(&rp);

    let method = class.methods.iter().skip(1).next();
    let Some(method) = method else { return; };

    let Some(attr) = method.attrs.first() else { return; };
    let classfile::attrs::AttrBody::Code((code_1, _code_2)) = &attr.body else { return; };
    let bytecode = &code_1.bytecode;
    for (pos, ix) in &bytecode.0 {
        // println!("IX: {:?}", ix);
        if let Instr::Ldc(pool_idx) = ix {
            // let item = class.cp.0.get(*pool_idx as usize);
            // let item = rp.ldc(*pool_idx as u16);
            let Some(const_line) = rp.cpool.get(*pool_idx as usize) else { continue; };
            let ConstData::Single(SingleTag::String, idx) = const_line.data else { continue; };
            let Some(const_line) = rp.cpool.get(idx as usize) else { continue; };
            let ConstData::Utf8(utf_data) = &const_line.data else { continue; };
            println!("LDC: {}", utf_data.s);
            // match item {
            //     RefOrString::Raw(idx) => {},
            //     RefOrString::Sym(idx) => {},
            //     RefOrString::RawBs(idx) => {},
            //     RefOrString::Str(str_lit) => {
            //         println!("STR LIT: {}", str_lit);
            //     },
            // };
            // println!("CP Item: {:?}", item);
            // if let Some(item) = class.cp.utf8(*pool_idx as u16) {
            //     if let Ok(text) = std::str::from_utf8(item) {
            //         println!("CP Item: {}", text);
            //     }
            // }
        }
    }
}

fn reasm(class: &Class<'_>) -> Result<Vec<u8>> {
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

fn patch_data(name: &str, data: &[u8]) -> Result<Vec<u8>> {
    let mut class = classfile::parse(
        &data,
        ParserOptions {
            no_short_code_attr: true,
        },
    )
    .map_err(|err| anyhow!("Parse: {:?}", err))?;

    if name.ends_with("kek.class") {
        trace_class(&class);
    }

    patch_class(&mut class);

    Ok(reasm(&class)?)
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
                println!("Patching integrity check");
                *ix = Instr::Sipush(0);
            }
        }
        bytecode.0 = new_bytecode;
    }
}
