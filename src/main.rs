use std::{env, fs, io::Read, path::Path, time::Instant, collections::HashSet};

use anyhow::{anyhow, Result};

use indicatif::ProgressBar;
use krakatau2::{
    file_output_util::Writer,
    lib::{
        assemble,
        classfile::{
            self,
            attrs::{AttrBody, Attribute},
            code::{Instr, Pos},
            parse::Class, cpool::{ConstPool, Const, BStr},
        },
        disassemble::refprinter::{
            self, ConstData, FmimTag, RefOrString, RefPrinter, SingleTag, UtfData,
        },
        AssemblerOptions, DisassemblerOptions, ParserOptions,
    },
    zip,
};

const ANCHOR: &str = "Inverted Selected Borderless Button background";

fn main() -> Result<()> {
    let args: Vec<String> = env::args().collect();

    let input_jar = &args[1];
    let output_jar = &args[2];

    let mut class_buf = Vec::new();
    let file = fs::File::open(input_jar)?;
    let mut zip = zip::ZipArchive::new(file)?;
    let class_ext = ".class";

    let now = Instant::now();

    let mut classes = vec![];

    let progress_bar = ProgressBar::new(zip.len() as u64);

    let rgb_method = {
        let mut file = zip.by_name("dsj.class").unwrap();
        class_buf.clear();
        class_buf.reserve(file.size() as usize);
        file.read_to_end(&mut class_buf)?;
        drop(file);
        find_rgb_method_in_data(&class_buf).unwrap()
    };

    for i in 0..zip.len() {
        progress_bar.inc(1);
        let mut file = zip.by_index(i)?;

        let name = file.name().to_owned();
        if !name.trim_end_matches('/').ends_with(&class_ext) {
            continue;
        }

        class_buf.clear();
        class_buf.reserve(file.size() as usize);
        file.read_to_end(&mut class_buf)?;

        if name.ends_with("dsj.class") || name.ends_with("oMz.class") || name.ends_with("theme/kX3.class") {
            let patched = patch_data(&name, &class_buf, &rgb_method)?;
            classes.push((name, patched));
        } else {
            classes.push((name, class_buf.clone()));
        }
    }

    progress_bar.finish();

    let dur = Instant::now().duration_since(now);
    println!("Patched: {:?}", dur);

    let mut writer = Writer::new(Path::new(output_jar))?;

    let now = Instant::now();

    for (name, data) in classes {
        writer.write(Some(&name), &data)?;
    }

    let dur = Instant::now().duration_since(now);
    println!("Writed: {:?}", dur);

    Ok(())
}

#[derive(Debug, Clone)]
struct MethodDescription {
    class: String,
    method: String,
    signature: String,
}

fn find_method_description(rp: &RefPrinter<'_>, method_id: u16) -> Option<MethodDescription> {
    let const_line = rp.cpool.get(method_id as usize)?;
    let ConstData::Fmim(FmimTag::Method, c, nat) = const_line.data else { return None; };

    let class = {
        let const_line = rp.cpool.get(c as usize)?;
        let ConstData::Single(SingleTag::Class, c) = const_line.data else { return None; };
        let const_line = rp.cpool.get(c as usize)?;
        let ConstData::Utf8(utf_data) = &const_line.data else { return None; };
        utf_data.s.to_string()
    };

    let const_line = rp.cpool.get(nat as usize)?;
    let ConstData::Nat(met, sig) = const_line.data else { return None; };

    let method = {
        let const_line = rp.cpool.get(met as usize)?;
        let ConstData::Utf8(utf_data) = &const_line.data else { return None; };
        utf_data.s.to_string()
    };

    let signature = {
        let const_line = rp.cpool.get(sig as usize)?;
        let ConstData::Utf8(utf_data) = &const_line.data else { return None; };
        utf_data.s.to_string()
    };

    Some(MethodDescription { class, method, signature })
}

fn find_utf_ldc(rp: &RefPrinter<'_>, id: u16) -> Option<String> {
    let const_line = rp.cpool.get(id as usize)?;
    let ConstData::Single(SingleTag::String, idx) = const_line.data else { return None; };
    let const_line = rp.cpool.get(idx as usize)?;
    let ConstData::Utf8(utf_data) = &const_line.data else { return None; };
    return Some(utf_data.s.to_string())
}

fn init_refprinter<'a>(cp: &ConstPool<'a>, attrs: &'a [Attribute<'a>]) -> RefPrinter<'a> {
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

    let rp = refprinter::RefPrinter::new(true, &cp, bstable, inner_classes);

    rp
}

type MethodId = u16;

fn find_rgb_method_in_data(data: &[u8]) -> Option<MethodDescription> {
    let class = classfile::parse(
        &data,
        ParserOptions {
            no_short_code_attr: true,
        },
    )
    .map_err(|err| anyhow!("Parse: {:?}", err)).ok()?;
    let (_id, desc) = find_rgb_method(&class)?;
    Some(desc)
}

fn find_rgb_method(class: &Class<'_>) -> Option<(MethodId, MethodDescription)> {
    println!("Searching RGB method");

    const RGB_SIG_START: &str = "(Ljava/lang/String;III)";

    let rp = init_refprinter(&class.cp, &class.attrs);

    let method = class.methods.iter().skip(1).next();
    let method = method?;

    let attr = method.attrs.first()?;
    let classfile::attrs::AttrBody::Code((code_1, _code_2)) = &attr.body else { return None; };
    let bytecode = &code_1.bytecode;

    for (_pos, ix) in &bytecode.0 {
        if let Instr::Invokevirtual(method_id) = &ix {
            let method_descr = find_method_description(&rp, *method_id)?;
            if method_descr.signature.starts_with(RGB_SIG_START) {
                return Some((*method_id, method_descr));
            }
        }
    }

    None
}

fn randomize_class<'a>(name: &str, class: &mut Class<'a>, method_idx: usize, rgb_method_desc: &'a MethodDescription) {
    println!("Randomizing {}", name);

    let (rgb_method_id, rgb_method_desc) = match find_rgb_method(class) {
        Some(met) => met,
        None => {
            println!("Can't find RGB method, adding CP entries.");

            let class_utf_id = class.cp.0.len();
            class.cp.0.push(Const::Utf8(BStr(rgb_method_desc.class.as_bytes())));

            let method_utf_id = class.cp.0.len();
            class.cp.0.push(Const::Utf8(BStr(rgb_method_desc.method.as_bytes())));

            let sig_utf_id = class.cp.0.len();
            class.cp.0.push(Const::Utf8(BStr(rgb_method_desc.signature.as_bytes())));

            let class_id = class.cp.0.len();
            class.cp.0.push(Const::Class(class_utf_id as u16));

            let name_and_type_id = class.cp.0.len();
            class.cp.0.push(Const::NameAndType(method_utf_id as u16, sig_utf_id as u16));

            let method_id = class.cp.0.len();
            class.cp.0.push(Const::Method(class_id as u16, name_and_type_id as u16));

            (method_id as u16, rgb_method_desc.clone())
        }
    };

    println!("RGB METHOD: {} {:?}", rgb_method_id, rgb_method_desc);

    const COLOR_DEFINE_SIGS: &[(&str, usize)] = &[
        ("(Ljava/lang/String;I)", 1),
        ("(Ljava/lang/String;III)", 3),
        // ("(Ljava/lang/String;IIII)", 4),
        ("(Ljava/lang/String;FFF)", 3),
    ];

    let rp = init_refprinter(&class.cp, &class.attrs);

    let method = class.methods.iter_mut().skip(method_idx).next();
    let Some(method) = method else { return; };

    let Some(attr) = method.attrs.first_mut() else { return; };
    let classfile::attrs::AttrBody::Code((code_1, _code_2)) = &mut attr.body else { return; };
    let bytecode = &mut code_1.bytecode;

    let mut new_bytecode: Vec<(Pos, Instr)> = vec![];

    let mut pos_gen = 0;

    for (_pos, ix) in bytecode.0.drain(..) {
        // println!("POS: {:?} IX: {:?}", pos, ix);
        let should_replace = match &ix {
            Instr::Invokevirtual(method_id) => {
                if let Some(method_descr) = find_method_description(&rp, *method_id) {
                    // println!("{:?}", method_descr);
                    COLOR_DEFINE_SIGS.iter().find_map(|(sig, color_args)| method_descr.signature.starts_with(sig).then_some(color_args))
                } else {
                    None
                }
            },
            _ => None,
        };

        if let Some(color_args) = should_replace {
            for _ in 0..*color_args {
                new_bytecode.pop().unwrap();
            }
            for _ in 0..3 {
                let rn: u8 = rand::random();
                let new = (
                    Pos(pos_gen),
                    Instr::Sipush(rn as i16)
                );
                // println!("NEW: {:?}", new);
                new_bytecode.push(new);
                pos_gen += 1;
            }
            // let last_label = new_bytecode.last().unwrap().0.0;
            // let this_label = pos.0;
            new_bytecode.push((Pos(pos_gen), Instr::Invokevirtual(rgb_method_id)));
            pos_gen += 1;
            // println!("---");
        } else {
            new_bytecode.push((Pos(pos_gen), ix));
            pos_gen += 1;
        }

        // if let Instr::Ldc(pool_idx) = ix {
        // }
    }
    bytecode.0 = new_bytecode;

    for attr in &mut code_1.attrs {
        let classfile::attrs::AttrBody::LineNumberTable(table) = &mut attr.body else { continue; };
        table.clear();
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

fn patch_data(name: &str, data: &[u8], rgb_method_desc: &MethodDescription) -> Result<Vec<u8>> {
    let mut class = classfile::parse(
        &data,
        ParserOptions {
            no_short_code_attr: true,
        },
    )
    .map_err(|err| anyhow!("Parse: {:?}", err))?;

    if name.ends_with("dsj.class") || name.ends_with("kX3.class") {
        let skip = if name.ends_with("dsj.class") { 1 } else if name.ends_with("kX3.class") { 4 } else { 0 };
        randomize_class(name, &mut class, skip, rgb_method_desc);
        Ok(reasm(&class)?)
    } else if name.ends_with("oMz.class") {
        patch_class(name, &mut class);
        Ok(reasm(&class)?)
    } else {
        panic!("raositenars");
    }

}

fn patch_class(name: &str, class: &mut Class<'_>) {
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
                println!("Patching integrity check in {}", name);
                *ix = Instr::Sipush(0);
            }
        }
        bytecode.0 = new_bytecode;
    }
}
