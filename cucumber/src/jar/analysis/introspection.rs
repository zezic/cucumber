use krakatau2::lib::{
    classfile::{
        attrs::AttrBody,
        code::{Instr, Pos},
        parse::Class,
    },
    disassemble::refprinter::{ConstData, FmimTag, RefPrinter},
    parse_utf8,
};

use crate::jar::{
    core::assembly::init_refprinter,
    types::methods::{MethodDescription, MethodSignatureKind},
};

/// Structure representing a named color getter invocation
#[derive(Debug)]
pub struct NamedColorGetterInvocation {
    pub class: String,
    pub method: String,
    pub ldc_pos: Pos,
}

/// Find a method by its signature and name
pub fn find_method_by_sig(
    class: &Class<'_>,
    sig_start: &str,
    _method_name: &str,
) -> Option<(u16, MethodDescription)> {
    let rp = init_refprinter(&class.cp, &class.attrs);

    let method = class.methods.iter().skip(1).next();
    let method = method?;

    let attr = method.attrs.first()?;
    let krakatau2::lib::classfile::attrs::AttrBody::Code((code_1, _)) = &attr.body else {
        return None;
    };

    let bytecode = &code_1.bytecode;
    let invokes = bytecode
        .0
        .iter()
        .filter_map(|(_pos, ix)| match ix {
            krakatau2::lib::classfile::code::Instr::Invokevirtual(method_id) => Some(method_id),
            _ => None,
        })
        .collect::<Vec<_>>();

    let found = find_method(sig_start, None, None, &invokes, &rp)?;

    // Find method ID from the found method description
    for (_, ix) in &bytecode.0 {
        if let krakatau2::lib::classfile::code::Instr::Invokevirtual(method_id) = ix {
            if let Some(desc) = find_method_description(&rp, *method_id, None) {
                if desc == found {
                    return Some((*method_id, found));
                }
            }
        }
    }

    None
}

/// Find method description from a method ID using the RefPrinter
pub fn find_method_description(
    rp: &RefPrinter<'_>,
    method_id: u16,
    color_rec_name: Option<&str>,
) -> Option<MethodDescription> {
    let const_line = rp.cpool.get(method_id as usize)?;
    let ConstData::Fmim(FmimTag::Method, c, nat) = const_line.data else {
        return None;
    };

    let class = {
        let const_line = rp.cpool.get(c as usize)?;
        let ConstData::Single(
            krakatau2::lib::disassemble::refprinter::SingleTag::Class,
            class_name_idx,
        ) = const_line.data
        else {
            return None;
        };
        let const_line = rp.cpool.get(class_name_idx as usize)?;
        let ConstData::Utf8(utf_data) = &const_line.data else {
            return None;
        };
        utf_data.s.to_string()
    };

    let method = {
        let const_line = rp.cpool.get(nat as usize)?;
        let ConstData::Nat(method_name, _sig_name) = const_line.data else {
            return None;
        };
        let const_line = rp.cpool.get(method_name as usize)?;
        let ConstData::Utf8(utf_data) = &const_line.data else {
            return None;
        };
        utf_data.s.to_string()
    };

    let signature = {
        let const_line = rp.cpool.get(nat as usize)?;
        let ConstData::Nat(_method_name, sig_name) = const_line.data else {
            return None;
        };
        let const_line = rp.cpool.get(sig_name as usize)?;
        let ConstData::Utf8(utf_data) = &const_line.data else {
            return None;
        };
        utf_data.s.to_string()
    };

    let signature_kind = match signature.as_str() {
        // Legacy signatures (void methods)
        "(I)V" => Some(MethodSignatureKind::Si),
        "(III)V" => Some(MethodSignatureKind::Siii),
        "(IIII)V" => Some(MethodSignatureKind::Siiii),
        "(FFF)V" => Some(MethodSignatureKind::Sfff),
        "(FFFF)V" => Some(MethodSignatureKind::Ffff),
        "(DDDD)V" => Some(MethodSignatureKind::Dddd),

        // New evolved signatures (methods that take String name + params and return color objects)
        sig if sig.starts_with("(Ljava/lang/String;I)") => Some(MethodSignatureKind::Si),
        sig if sig.starts_with("(Ljava/lang/String;III)") => Some(MethodSignatureKind::Siii),
        sig if sig.starts_with("(Ljava/lang/String;IIII)") => Some(MethodSignatureKind::Siiii),
        sig if sig.starts_with("(Ljava/lang/String;FFF)") => Some(MethodSignatureKind::Sfff),
        sig if sig.starts_with("(Ljava/lang/String;FFFF)") => Some(MethodSignatureKind::Ffff),

        // Reference-based HSV methods
        sig if sig.starts_with("(Ljava/lang/String;L") && sig.contains("FFF)") => {
            if let Some(color_rec_name) = color_rec_name {
                if sig.contains(color_rec_name) {
                    Some(MethodSignatureKind::SRfff)
                } else {
                    Some(MethodSignatureKind::SSfff)
                }
            } else {
                Some(MethodSignatureKind::SSfff)
            }
        }

        // Legacy reference-based methods
        sig if sig.starts_with("(L") && sig.ends_with("FFF)V") => {
            if let Some(color_rec_name) = color_rec_name {
                if sig.contains(color_rec_name) {
                    Some(MethodSignatureKind::SRfff)
                } else {
                    Some(MethodSignatureKind::SSfff)
                }
            } else {
                Some(MethodSignatureKind::SSfff)
            }
        }

        _ => None,
    };

    Some(MethodDescription {
        class,
        method,
        signature,
        signature_kind,
    })
}

/// Find UTF-8 string from an LDC instruction
pub fn find_utf_ldc(rp: &RefPrinter<'_>, id: u16) -> Option<String> {
    let const_line = rp.cpool.get(id as usize)?;
    let ConstData::Single(krakatau2::lib::disassemble::refprinter::SingleTag::String, idx) =
        const_line.data
    else {
        return None;
    };
    let const_line = rp.cpool.get(idx as usize)?;
    let ConstData::Utf8(utf_data) = &const_line.data else {
        return None;
    };
    return Some(utf_data.s.to_string());
}

/// Find constant name from a field reference
pub fn find_const_name(rp: &RefPrinter<'_>, id: u16) -> Option<String> {
    let const_line = rp.cpool.get(id as usize)?;

    let ConstData::Fmim(FmimTag::Field, _c, nat) = const_line.data else {
        return None;
    };

    let const_line = rp.cpool.get(nat as usize)?;
    let ConstData::Nat(const_name, _class_name) = const_line.data else {
        return None;
    };

    let const_line = rp.cpool.get(const_name as usize)?;
    let ConstData::Utf8(utf_data) = &const_line.data else {
        return None;
    };

    Some(utf_data.s.to_string())
}

/// Extract named color getter method description from a class
pub fn extract_named_color_getter_1(
    class: &Class,
    raw_color_class: &str,
) -> Option<MethodDescription> {
    let sig_start = format!("(Ljava/lang/String;)L{};", raw_color_class);
    for field in &class.methods {
        let descriptor = class.cp.utf8(field.desc).and_then(parse_utf8)?;
        if descriptor.starts_with(&sig_start) {
            let class_name = class.cp.clsutf(class.this).and_then(parse_utf8)?;
            let method = class.cp.utf8(field.name).and_then(parse_utf8)?;

            return Some(MethodDescription {
                class: class_name,
                method,
                signature: descriptor,
                signature_kind: None,
            });
        }
    }

    None
}

/// Find named color getter invocations in a class
pub fn find_named_color_getter_1_invocations(
    class: &Class,
    named_color_getter: &MethodDescription,
) -> Vec<(String, NamedColorGetterInvocation)> {
    let rp = init_refprinter(&class.cp, &class.attrs);
    let mut results = Vec::new();

    let class_name = class
        .cp
        .clsutf(class.this)
        .and_then(parse_utf8)
        .unwrap_or_default();

    for method in &class.methods {
        let method_name = class
            .cp
            .utf8(method.name)
            .and_then(parse_utf8)
            .unwrap_or_default();

        let Some(attr) = method.attrs.first() else {
            continue;
        };
        let AttrBody::Code((code_1, _)) = &attr.body else {
            continue;
        };

        let bytecode = &code_1.bytecode;

        // First, find all invocations of the target method
        let mut invocation_positions = Vec::new();
        for (pos, instr) in &bytecode.0 {
            if let Instr::Invokevirtual(method_id) = instr {
                if let Some(method_descr) = find_method_description(&rp, *method_id, None) {
                    if method_descr.class == named_color_getter.class
                        && method_descr.method == named_color_getter.method
                        && method_descr.signature == named_color_getter.signature
                    {
                        invocation_positions.push(*pos);
                    }
                }
            }
        }

        // For each invocation, find the direct preceding Ldc and any jump branches
        for invocation_pos in invocation_positions {
            let mut results_for_invocation = Vec::new();

            // Find direct preceding Ldc
            let mut direct_ldc = None;
            for (pos, instr) in bytecode.0.iter().rev() {
                if *pos >= invocation_pos {
                    continue;
                }
                if let Instr::Ldc(id) = instr {
                    if let Some(color_name) = find_utf_ldc(&rp, *id as u16) {
                        direct_ldc = Some((*pos, color_name));
                        break;
                    }
                }
            }

            // Find immediate preceding jump that targets this invocation
            let mut jump_to_invocation = None;
            for (pos, instr) in bytecode.0.iter().rev() {
                if *pos >= invocation_pos {
                    continue;
                }
                match instr {
                    Instr::Goto(target) | Instr::GotoW(target) => {
                        if *target == invocation_pos {
                            jump_to_invocation = Some(*pos);
                            break;
                        }
                    }
                    _ => {}
                }
            }

            // If we have a direct Ldc, add it
            if let Some((ldc_pos, color_name)) = direct_ldc {
                results_for_invocation.push((ldc_pos, color_name));
            }

            // If we have a jump, find the first Ldc before that jump
            if let Some(jump_pos) = jump_to_invocation {
                for (pos, instr) in bytecode.0.iter().rev() {
                    if *pos >= jump_pos {
                        continue;
                    }
                    if let Instr::Ldc(id) = instr {
                        if let Some(color_name) = find_utf_ldc(&rp, *id as u16) {
                            results_for_invocation.push((*pos, color_name));
                            break;
                        }
                    }
                }
            }

            // Add all found strings for this invocation
            for (ldc_pos, color_name) in results_for_invocation {
                results.push((
                    color_name,
                    NamedColorGetterInvocation {
                        class: class_name.clone(),
                        method: method_name.clone(),
                        ldc_pos,
                    },
                ));
            }
        }
    }

    results
}

/// Find a method with specific signature and optional parameters
fn find_method(
    signature_start: &str,
    color_rec_name: Option<&str>,
    skip: Option<usize>,
    invokes: &[&u16],
    rp: &RefPrinter<'_>,
) -> Option<MethodDescription> {
    let skip = skip.unwrap_or(0);
    for method_id in invokes.iter().skip(skip) {
        let desc = find_method_description(rp, **method_id, color_rec_name)?;
        if desc.signature.starts_with(signature_start) {
            return Some(desc);
        }
    }
    None
}
