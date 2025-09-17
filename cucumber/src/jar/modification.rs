use krakatau2::lib::classfile::{
    attrs::AttrBody,
    code::{Instr, Pos},
    cpool::{BStr, Const},
    parse::Class,
};
use tracing::{debug, warn};

use crate::{
    jar::{
        analysis::{find_method_by_sig, find_method_description, find_utf_ldc, init_refprinter},
        goodies::{ColorComponents, NamedColor, PaletteColorMethods},
    },
    types::CompositingMode,
};

pub fn replace_named_color<'a>(
    class: &mut Class<'a>,
    name: &str,
    new_value: ColorComponents,
    named_colors: &mut [NamedColor],
    palette_color_meths: &'a PaletteColorMethods,
    compositing_mode: CompositingMode,
) -> Option<()> {
    if !matches!(
        new_value,
        ColorComponents::Rgbai(..) | ColorComponents::DeltaHsvf(..)
    ) {
        todo!("Only Rgbai and Hsvf supported for the moment");
    }

    if matches!(compositing_mode, CompositingMode::RelativeToBackground) {
        warn!("Relative compositing is not supported yet: {}", name);
        return None;
    }

    let named_color = named_colors
        .iter_mut()
        .find(|color| color.color_name == name)?;

    debug!("### REPLACING {}: {:?}", name, new_value);

    let method_descr_to_find = match compositing_mode {
        CompositingMode::BlendedOnBackground => &palette_color_meths.rgba_i_blended_on_background,
        CompositingMode::RelativeToBackground => &palette_color_meths.hsv_f_relative_to_background,
        CompositingMode::Absolute => &palette_color_meths.rgba_i_absolute,
    };

    let (rgbai_method_id, _rgbai_method_desc) = match find_method_by_sig(
        class,
        &method_descr_to_find.signature,
        &method_descr_to_find.method,
    ) {
        Some(met) => met,
        None => {
            let rgbai_method_desc = &palette_color_meths.rgba_i_absolute;

            let consts = &mut class.cp.0;

            let class_utf_id = consts.len();
            consts.push(Const::Utf8(BStr(rgbai_method_desc.class.as_bytes())));

            let method_utf_id = consts.len();
            consts.push(Const::Utf8(BStr(rgbai_method_desc.method.as_bytes())));

            let sig_utf_id = consts.len();
            consts.push(Const::Utf8(BStr(rgbai_method_desc.signature.as_bytes())));

            let class_id = consts.len();
            consts.push(Const::Class(class_utf_id as u16));

            let name_and_type_id = consts.len();
            consts.push(Const::NameAndType(method_utf_id as u16, sig_utf_id as u16));

            let method_id = consts.len();
            consts.push(Const::Method(class_id as u16, name_and_type_id as u16));

            (method_id as u16, rgbai_method_desc.clone())
        }
    };

    let rp = init_refprinter(&class.cp, &class.attrs);

    let old_desc = palette_color_meths.from_components(&named_color.components);

    let method = class.methods.get_mut(named_color.method_idx)?;

    let attr = method.attrs.first_mut()?;

    let AttrBody::Code((code_1, _code_2)) = &mut attr.body else {
        return None;
    };
    if code_1.stack < 7 {
        code_1.stack = 7;
    }
    let bytecode = &mut code_1.bytecode;
    let mut old_bytecode = bytecode.0.drain(..);
    let mut new_bytecode: Vec<(Pos, Instr)> = vec![];
    let mut pos_gen = 0..;

    let mut ready = false;

    while let Some((_, ix)) = old_bytecode.next() {
        new_bytecode.push((Pos(pos_gen.next()?), ix));
        if ready {
            continue;
        }

        let id = match new_bytecode.last()?.1 {
            Instr::Ldc(id) => id as u16,
            Instr::LdcW(id) => id as u16,
            _ => {
                continue;
            }
        };

        let Some(text) = find_utf_ldc(&rp, id as u16) else {
            continue;
        };
        if text == name {
            loop {
                let ix = old_bytecode.next().unwrap();
                if let Instr::Invokevirtual(method_id) = ix.1 {
                    let desc = find_method_description(&rp, method_id, None).unwrap();
                    if desc.signature == old_desc.signature {
                        break;
                    }
                }
            }
            let (ixs_to_push, floats_to_add) = new_value.to_ixs(class.cp.0.len());
            for ix in ixs_to_push {
                new_bytecode.push((Pos(pos_gen.next()?), ix));
            }
            if let Some(floats) = floats_to_add {
                for float in floats {
                    class
                        .cp
                        .0
                        .push(Const::Float(u32::from_be_bytes(float.float.to_be_bytes())));
                }
            }

            // Now invoke correct method instead of old
            new_bytecode.push((Pos(pos_gen.next()?), Instr::Invokevirtual(rgbai_method_id)));
            named_color.components = new_value.clone();
            ready = true;
        }
    }
    drop(old_bytecode);

    bytecode.0 = new_bytecode;

    for attr in &mut code_1.attrs {
        let AttrBody::LineNumberTable(table) = &mut attr.body else {
            continue;
        };
        table.clear();
    }

    Some(())
}
