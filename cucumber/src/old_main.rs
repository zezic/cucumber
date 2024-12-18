use std::{env, fs, io::Read, path::Path, time::Instant, collections::HashMap};

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
            self, ConstData, FmimTag, RefPrinter, SingleTag, PrimTag,
        },
        AssemblerOptions, DisassemblerOptions, ParserOptions,
    },
    zip,
};

mod ask;
mod mapping;

fn main() -> Result<()> {
    let args: Vec<String> = env::args().collect();

    let input_jar = &args[1];
    let output_jar = &args[2];

    let ask_file = &args[3];

    debug!("ASK: {}", ask_file);

    let ableton_color_defs = ask::parse_ask(&ask_file).unwrap();
    let mut html = String::new();
    for (name, (r, g, b, a)) in &ableton_color_defs {
        let def = ColorDef {
            name: name.clone(),
            color: Color::Rgbau(*r, *g, *b, *a)
        };
        let def_html = def.as_html();
        html.push_str(&format!("{def_html}\n"));
    }
    fs::write("abl_theme.html", &html).expect("Unable to write theme file");

    let mut bw_abl_mapping: HashMap<&str, (u8, u8, u8, u8)> = HashMap::new();
    for (bw_name, abl_name) in mapping::RAW_MAPPING {
        if let Some(def) = ableton_color_defs.get(&abl_name.to_string()) {
            bw_abl_mapping.insert(bw_name, *def);
        } else {
            panic!("Can't find color in Ableton theme: {}", abl_name);
        }
    }

    let mut class_buf = Vec::new();
    let file = fs::File::open(input_jar)?;
    let mut zip = zip::ZipArchive::new(file)?;
    let class_ext = ".class";

    let now = Instant::now();

    let mut classes = vec![];

    let progress_bar = ProgressBar::new(zip.len() as u64);

    let rgba_method = {
        let mut file = zip.by_name("daz.class").unwrap();
        class_buf.clear();
        class_buf.reserve(file.size() as usize);
        file.read_to_end(&mut class_buf)?;
        drop(file);
        find_rgba_method_in_data(&class_buf).unwrap()
    };

    let mut html = String::new();

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

        if name.ends_with("daz.class") ||
            name.ends_with("myf.class") ||
            name.ends_with("theme/irK.class") {
            let patched = patch_data(&name, &class_buf, &rgba_method, &mut html, &bw_abl_mapping)?;
            classes.push((name, patched));
        } else {
            classes.push((name, class_buf.clone()));
        }
    }

    progress_bar.finish();

    let dur = Instant::now().duration_since(now);
    debug!("Patched: {:?}", dur);

    let mut writer = Writer::new(Path::new(output_jar))?;

    let now = Instant::now();

    for (name, data) in classes {
        writer.write(Some(&name), &data)?;
    }

    let dur = Instant::now().duration_since(now);
    debug!("Writed: {:?}", dur);

    fs::write("bw_theme.html", &html).expect("Unable to write theme file");

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

fn find_float_ldc(rp: &RefPrinter<'_>, id: u16) -> Option<f32> {
    let const_line = rp.cpool.get(id as usize)?;
    let ConstData::Prim(PrimTag::Float, float_str) = &const_line.data else { return None; };
    float_str.trim_end_matches("f").parse::<f32>().ok()
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

fn find_rgba_method_in_data(data: &[u8]) -> Option<MethodDescription> {
    let class = classfile::parse(
        &data,
        ParserOptions {
            no_short_code_attr: true,
        },
    )
    .map_err(|err| anyhow!("Parse: {:?}", err)).ok()?;
    let (_id, desc) = find_method_by_sig(&class, "(Ljava/lang/String;IIII)")?;
    Some(desc)
}

fn find_method_by_sig(class: &Class<'_>, sig_start: &str) -> Option<(MethodId, MethodDescription)> {
    debug!("Searching RGB method");

    let rp = init_refprinter(&class.cp, &class.attrs);

    let method = class.methods.iter().skip(1).next();
    let method = method?;

    let attr = method.attrs.first()?;
    let classfile::attrs::AttrBody::Code((code_1, _code_2)) = &attr.body else { return None; };
    let bytecode = &code_1.bytecode;

    for (_pos, ix) in &bytecode.0 {
        if let Instr::Invokevirtual(method_id) = &ix {
            let method_descr = find_method_description(&rp, *method_id)?;
            if method_descr.signature.starts_with(sig_start) {
                return Some((*method_id, method_descr));
            }
        }
    }

    None
}

#[derive(Debug, Clone)]
enum Color {
    Rgbu(u8, u8, u8),
    #[allow(dead_code)]
    HsvfAdjustment(f32, f32, f32),
    Rgbau(u8, u8, u8, u8),
    Grayscale(u8),
}

#[derive(Debug, Clone)]
struct ColorDef {
    name: String,
    color: Color,
}

impl ColorDef {
    fn as_html(&self) -> String {
        let color_style = match self.color {
            Color::Rgbu(r, g, b) => format!("rgb({r}, {g}, {b})"),
            Color::HsvfAdjustment(..) => format!("gray"),
            Color::Rgbau(r, g, b, a) => {
                let a_f = a as f32 / 255.0;
                format!("rgba({r}, {g}, {b}, {a_f})")
            },
            Color::Grayscale(v) => format!("rgb({v}, {v}, {v})"),
        };
        let name = &self.name;
        let stripes = "background: repeating-linear-gradient(45deg, #000000, #000000 10px, #ffffff 10px, #ffffff 20px);";
        format!("<div style='display: flex;'><div style='{stripes}'><div style='background-color: {color_style}; height: 40px; width: 80px;'></div></div>{name}</div>")
    }
}

#[derive(Eq, PartialEq)]
enum ColorMethod {
    Rgbu,
    HsvfAdjustment,
    Rgbau,
    Grayscale,
}

fn instr_to_float(instr: &Instr, rp: &RefPrinter<'_>) -> f32 {
    match instr {
        Instr::Ldc(id) => {
            find_float_ldc(&rp, *id as u16).unwrap()
        }
        Instr::Fconst0 => 0.0,
        Instr::Fconst1 => 1.0,
        Instr::Fconst2 => 2.0,
        _ => unreachable!("Unexpected IX for float")
    }
}

fn instr_to_u8(instr: &Instr) -> u8 {
    match instr {
        Instr::Iconst0 => 0,
        Instr::Iconst1 => 1,
        Instr::Iconst2 => 2,
        Instr::Iconst3 => 3,
        Instr::Iconst4 => 4,
        Instr::Iconst5 => 5,
        Instr::Bipush(num) => *num as u8,
        Instr::Sipush(num) => *num as u8,
        _ => unreachable!("Unexpected IX for u8")
    }
}

fn colorize_class<'a>(name: &str, class: &mut Class<'a>, method_idx: usize, rgba_method_desc: &'a MethodDescription, bw_abl_mapping: &HashMap<&str, (u8, u8, u8, u8)>) -> Result<Vec<ColorDef>> {
    debug!("Colorizing {}", name);
    let mut color_defs = vec![];

    let (rgba_method_id, rgba_method_desc) = match find_method_by_sig(class, "(Ljava/lang/String;IIII)") {
        Some(met) => met,
        None => {
            debug!("Can't find RGBA method, adding CP entries.");

            let class_utf_id = class.cp.0.len();
            class.cp.0.push(Const::Utf8(BStr(rgba_method_desc.class.as_bytes())));

            let method_utf_id = class.cp.0.len();
            class.cp.0.push(Const::Utf8(BStr(rgba_method_desc.method.as_bytes())));

            let sig_utf_id = class.cp.0.len();
            class.cp.0.push(Const::Utf8(BStr(rgba_method_desc.signature.as_bytes())));

            let class_id = class.cp.0.len();
            class.cp.0.push(Const::Class(class_utf_id as u16));

            let name_and_type_id = class.cp.0.len();
            class.cp.0.push(Const::NameAndType(method_utf_id as u16, sig_utf_id as u16));

            let method_id = class.cp.0.len();
            class.cp.0.push(Const::Method(class_id as u16, name_and_type_id as u16));

            (method_id as u16, rgba_method_desc.clone())
        }
    };

    debug!("RGBA METHOD: {} {:?}", rgba_method_id, rgba_method_desc);

    const COLOR_DEFINE_SIGS: &[(&str, usize, ColorMethod)] = &[
        ("(Ljava/lang/String;I)", 1, ColorMethod::Grayscale),
        ("(Ljava/lang/String;III)", 3, ColorMethod::Rgbu),
        ("(Ljava/lang/String;IIII)", 4, ColorMethod::Rgbau),
        ("(Ljava/lang/String;FFF)", 3, ColorMethod::HsvfAdjustment),
    ];

    let rp = init_refprinter(&class.cp, &class.attrs);

    let method = class.methods.iter_mut().skip(method_idx).next();
    let Some(method) = method else { return Err(anyhow!("No method at offset {}", method_idx)); };

    let Some(attr) = method.attrs.first_mut() else { return Err(anyhow!("No first attr in method")); };
    let classfile::attrs::AttrBody::Code((code_1, _code_2)) = &mut attr.body else { return Err(anyhow!("Attr body is not Code")); };
    let bytecode = &mut code_1.bytecode;

    let mut new_bytecode: Vec<(Pos, Instr)> = vec![];

    let mut pos_gen = 0;

    for (_pos, ix) in bytecode.0.drain(..) {
        let can_replace = match &ix {
            Instr::Invokevirtual(method_id) => {
                if let Some(method_descr) = find_method_description(&rp, *method_id) {
                    COLOR_DEFINE_SIGS.iter().find_map(|(sig, color_args, color_method)| method_descr.signature.starts_with(sig).then_some((color_args, color_method)))
                } else {
                    None
                }
            },
            _ => None,
        };

        if let Some((color_args, color_method)) = can_replace {
            let maybe_ldc = &new_bytecode[new_bytecode.len() - 1 - color_args];
            let Instr::Ldc(id) = maybe_ldc.1 else { panic!("No name LDC for color ix") };
            let ldc = find_utf_ldc(&rp, id as u16).unwrap();

            let color_def = ColorDef {
                name: ldc.clone(),
                color: match color_method {
                    ColorMethod::Rgbu => {
                        let r = instr_to_u8(&new_bytecode[new_bytecode.len() - 1 - color_args + 1].1);
                        let g = instr_to_u8(&new_bytecode[new_bytecode.len() - 1 - color_args + 2].1);
                        let b = instr_to_u8(&new_bytecode[new_bytecode.len() - 1 - color_args + 3].1);
                        Color::Rgbu(r, g, b)
                    },
                    ColorMethod::HsvfAdjustment => {
                        let h = instr_to_float(&new_bytecode[new_bytecode.len() - 1 - color_args + 1].1, &rp);
                        let s = instr_to_float(&new_bytecode[new_bytecode.len() - 1 - color_args + 1].1, &rp);
                        let v = instr_to_float(&new_bytecode[new_bytecode.len() - 1 - color_args + 1].1, &rp);
                        Color::HsvfAdjustment(h, s, v)
                    },
                    ColorMethod::Rgbau => {
                        let r = instr_to_u8(&new_bytecode[new_bytecode.len() - 1 - color_args + 1].1);
                        let g = instr_to_u8(&new_bytecode[new_bytecode.len() - 1 - color_args + 2].1);
                        let b = instr_to_u8(&new_bytecode[new_bytecode.len() - 1 - color_args + 3].1);
                        let a = instr_to_u8(&new_bytecode[new_bytecode.len() - 1 - color_args + 4].1);
                        Color::Rgbau(r, g, b, a)
                    },
                    ColorMethod::Grayscale => {
                        let v = instr_to_u8(&new_bytecode[new_bytecode.len() - 1 - color_args + 1].1);
                        Color::Grayscale(v)
                    }
                }
            };
            color_defs.push(color_def.clone());

            if let Some(new_colors) = bw_abl_mapping.get(ldc.as_str()) {
                let (r, g, b, a) = *new_colors;
                let colors = [r, g, b, a];

                for _ in 0..*color_args {
                    new_bytecode.pop();
                }

                for color in colors {
                    let new = (
                        Pos(pos_gen),
                        Instr::Sipush(color as i16)
                    );
                    new_bytecode.push(new);
                    pos_gen += 1;
                }
                new_bytecode.push((Pos(pos_gen), Instr::Invokevirtual(rgba_method_id)));
                pos_gen += 1;
            } else {
                new_bytecode.push((Pos(pos_gen), ix));
                pos_gen += 1;
            }
        } else {
            new_bytecode.push((Pos(pos_gen), ix));
            pos_gen += 1;
        }
    }

    bytecode.0 = new_bytecode;

    for attr in &mut code_1.attrs {
        let classfile::attrs::AttrBody::LineNumberTable(table) = &mut attr.body else { continue; };
        table.clear();
    }

    Ok(color_defs)
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

fn patch_data(name: &str, data: &[u8], rgba_method_desc: &MethodDescription, html: &mut String, bw_abl_mapping: &HashMap<&str, (u8, u8, u8, u8)>) -> Result<Vec<u8>> {
    let mut class = classfile::parse(
        &data,
        ParserOptions {
            no_short_code_attr: true,
        },
    )
    .map_err(|err| anyhow!("Parse: {:?}", err))?;

    if name.ends_with("daz.class") || name.ends_with("irK.class") {
        let skip = if name.ends_with("daz.class") { 1 } else if name.ends_with("irK.class") { 4 } else { 0 };
        let color_defs = colorize_class(name, &mut class, skip, rgba_method_desc, bw_abl_mapping).unwrap();
        for def in color_defs {
            let def_html = def.as_html();
            html.push_str(&format!("{def_html}\n"));
        }
        Ok(reasm(&class)?)
    } else if name.ends_with("myf.class") {
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
                debug!("Patching integrity check in {}", name);
                *ix = Instr::Sipush(0);
            }
        }
        bytecode.0 = new_bytecode;
    }
}
