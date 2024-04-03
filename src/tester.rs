use std::{collections::HashSet, env, fmt::Debug, fs, io::Read};

use anyhow::anyhow;

use indicatif::ProgressBar;
use krakatau2::{
    lib::{
        classfile::{
            self, attrs::{AttrBody, Attribute}, code::Instr, cpool::ConstPool, parse::Class
        },
        disassemble::refprinter::{ConstData, FmimTag, RefPrinter, SingleTag},
        ParserOptions,
    },
    zip::{self, read::ZipFile},
};

// Will search constant pool for that (inside Utf8 entry)
// Contain most of the colors and methods to set them
const PALETTE_ANCHOR: &str = "Device Tint Future";
// Contain time-bomb initialization around constant 5000
const INIT_ANCHOR: &str = "Apply Device Remote Control Changes To All Devices";

fn main() -> anyhow::Result<()> {
    let args: Vec<String> = env::args().collect();
    let input_jar = &args[1];

    let file = fs::File::open(input_jar)?;
    let mut zip = zip::ZipArchive::new(file)?;

    let file_names = zip.file_names().map(Into::into).collect::<Vec<String>>();
    const PARSER_OPTIONS: ParserOptions = ParserOptions {
        no_short_code_attr: true,
    };

    let mut palette_color_meths = None;

    let mut data = Vec::new();

    let progress_bar = ProgressBar::new(file_names.len() as u64);
    for file_name in &file_names {
        let mut file = zip.by_name(file_name).unwrap();

        data.clear();
        file.read_to_end(&mut data)?;

        let Ok(class) = classfile::parse(&data, PARSER_OPTIONS) else {
            continue;
        };

        if let Some(useful_file_type) = is_useful_file(&class) {
            match useful_file_type {
                UsefulFileType::MainPalette => {
                    if let Some(methods) = extract_palette_color_methods(&class) {
                        println!("{:#?}", methods);
                        palette_color_meths = Some(methods);
                    }
                },
                UsefulFileType::Init => {
                    println!("Found init")
                },
            }
        }
        progress_bar.inc(1);
        drop(file);
    }
    progress_bar.finish();

    if let Some(palette_color_meths) = palette_color_meths {
        let progress_bar = ProgressBar::new(file_names.len() as u64);
        for file_name in file_names {
            let mut file = zip.by_name(&file_name).unwrap();

            data.clear();
            file.read_to_end(&mut data)?;

            let Ok(class) = classfile::parse(&data, PARSER_OPTIONS) else {
                continue;
            };

            scan_for_color_defs(&class, &palette_color_meths, &file_name);
            progress_bar.inc(1);
            drop(file);
        }
        progress_bar.finish();
    }

    // let mut file = zip.by_name("dsj.class").unwrap();
    // let mut data = Vec::new();
    // data.clear();
    // data.reserve(file.size() as usize);
    // file.read_to_end(&mut data)?;
    // drop(file);

    // let class = classfile::parse(
    //     &data,
    //     ParserOptions {
    //         no_short_code_attr: true,
    //     },
    // ).map_err(|err| anyhow!("Parse: {:?}", err))?;

    // for entry in class.cp.0 {
    //     if let classfile::cpool::Const::Utf8(txt) = entry {
    //         println!("{:?}", txt);
    //     }
    // }

    Ok(())
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct MethodDescription {
    class: String,
    method: String,
    signature: String,
    signature_kind: Option<MethodSignatureKind>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum MethodSignatureKind {
    Si,
    Siii,
    Siiii,
    Sfff,
    SRfff, // R - reference to other, already defined color
    SSfff,
}

impl MethodSignatureKind {
    fn color_name_ix_offset(&self) -> usize {
        match self {
            MethodSignatureKind::Si => 2,
            MethodSignatureKind::Siii => 4,
            MethodSignatureKind::Siiii => 5,
            MethodSignatureKind::Sfff => 4,
            MethodSignatureKind::SRfff => 6,
            MethodSignatureKind::SSfff => 5,
        }
    }
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

    let rp = RefPrinter::new(true, &cp, bstable, inner_classes);

    rp
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

    let signature_kind = if let Some((sig_start, _)) = signature.split_once(")") {
        use MethodSignatureKind::*;
        match sig_start {
            "(Ljava/lang/String;I" => Some(Si),
            "(Ljava/lang/String;III" => Some(Siii),
            "(Ljava/lang/String;IIII" => Some(Siiii),
            "(Ljava/lang/String;FFF" => Some(Sfff),
            "(Ljava/lang/String;LduR;FFF" => Some(SRfff),
            "(Ljava/lang/String;Ljava/lang/String;FFF" => Some(SSfff),
            _ => None,
        }
    } else {
        None
    };

    Some(MethodDescription { class, method, signature, signature_kind })
}

fn find_utf_ldc(rp: &RefPrinter<'_>, id: u16) -> Option<String> {
    let const_line = rp.cpool.get(id as usize)?;
    let ConstData::Single(SingleTag::String, idx) = const_line.data else { return None; };
    let const_line = rp.cpool.get(idx as usize)?;
    let ConstData::Utf8(utf_data) = &const_line.data else { return None; };
    return Some(utf_data.s.to_string())
}

fn scan_for_color_defs(class: &Class, palette_color_meths: &PaletteColorMethods, filename: &str) {
    let rp = init_refprinter(&class.cp, &class.attrs);

    let all_meths = palette_color_meths.all();

    for method in &class.methods {
        let Some(attr) = method.attrs.first() else { continue; };
        let AttrBody::Code((code_1, _)) = &attr.body else {
            continue;
        };

        let bytecode = &code_1.bytecode;

        for (idx, (_, ix)) in bytecode.0.iter().enumerate() {
            let Instr::Invokevirtual(method_id) = ix else { continue; };
            let Some(method_descr) = find_method_description(&rp, *method_id) else { continue; };

            for meth in &all_meths {
                if method_descr == **meth {
                    if let Some(sig_kind) = &meth.signature_kind {
                        let offset = sig_kind.color_name_ix_offset();
                        let Some((_, ix)) = bytecode.0.get(idx - offset) else {
                            println!("{}: offset out of bounds", filename);
                            continue;
                        };
                        match ix {
                            Instr::Ldc(id) => {
                                let text = find_utf_ldc(&rp, *id as u16);
                                println!("{}: {:?}", filename, text);
                            },
                            other => {
                                println!("{}: {:?}", filename, other);
                            }
                        }
                    } else {
                        println!("No signature kind prepared :(");
                    }
                }
            }
        }
    }
}

#[derive(Debug)]
enum UsefulFileType {
    MainPalette,
    Init,
}

fn is_useful_file(class: &Class) -> Option<UsefulFileType> {
    let mtch = has_any_string_in_constant_pool(class, &[PALETTE_ANCHOR, INIT_ANCHOR])?;

    let useful_file_type = match mtch {
        PALETTE_ANCHOR => UsefulFileType::MainPalette,
        INIT_ANCHOR => UsefulFileType::Init,
        _ => return None,
    };

    Some(useful_file_type)
}

#[derive(Debug)]
struct PaletteColorMethods {
    grayscale_i: MethodDescription,
    rgb_i: MethodDescription,
    rgba_i: MethodDescription,
    rgb_f: MethodDescription,
    ref_hsv_f: Option<MethodDescription>,
    name_hsv_f: MethodDescription,
}

impl PaletteColorMethods {
    fn all(&self) -> Vec<&MethodDescription> {
        let mut out = vec![
            &self.grayscale_i,
            &self.rgb_i,
            &self.rgba_i,
            &self.rgb_f,
            &self.name_hsv_f,
        ];
        if let Some(meth) = &self.ref_hsv_f {
            out.push(meth);
        }
        out
    }
}

fn extract_palette_color_methods(class: &Class) -> Option<PaletteColorMethods> {
    println!("Searching color methods");

    let rp = init_refprinter(&class.cp, &class.attrs);

    let main_palette_method = class.methods.iter().skip(1).next()?;
    let attr = main_palette_method.attrs.first()?;
    let AttrBody::Code((code_1, _)) = &attr.body else {
        return None;
    };

    let bytecode = &code_1.bytecode;

    let invokes = bytecode.0.iter().filter_map(|(_, ix)| match ix {
        Instr::Invokevirtual(method_id) => Some(method_id),
        _ => None
    });

    let find_method = |signature_start: &str| {
        let mut invokes = invokes.clone();
        invokes.find_map(|method_id| {
            let method_descr = find_method_description(&rp, *method_id)?;
            if method_descr.signature.starts_with(signature_start) {
                Some(method_descr)
            } else {
                None
            }
        })
    };

    let grayscale_i = find_method("(Ljava/lang/String;I)")?;
    let rgb_i = find_method("(Ljava/lang/String;III)")?;
    let rgba_i = find_method("(Ljava/lang/String;IIII)")?;
    let rgb_f = find_method("(Ljava/lang/String;FFF)")?;
    let ref_hsv_f = find_method("(Ljava/lang/String;LduR;FFF)");
    let name_hsv_f = find_method("(Ljava/lang/String;Ljava/lang/String;FFF)")?;

    Some(PaletteColorMethods { grayscale_i, rgb_i, rgba_i, rgb_f, ref_hsv_f, name_hsv_f })
}

fn has_any_string_in_constant_pool<'a>(class: &Class, strings: &[&'a str]) -> Option<&'a str> {
    for entry in &class.cp.0 {
        if let classfile::cpool::Const::Utf8(txt) = entry {
            let parsed_string = String::from_utf8_lossy(txt.0);
            if let Some(found) = strings.iter().find(|pattern| **pattern == parsed_string) {
                return Some(found);
            }
        }
    }

    None
}
