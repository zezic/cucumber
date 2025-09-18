use krakatau2::lib::classfile;

// Will search constant pool for that (inside Utf8 entry)
// Contain most of the colors and methods to set them
const PALETTE_ANCHOR: &str = "Device Tint Future";

// Contain time-bomb initialization around constant 5000
const INIT_ANCHOR: &str = "Apply Device Remote Control Changes To All Devices";

// Contain crash report builder where we can get some info about release version
const CRASH_REPORT_ANCHOR: &str = "stack trace.txt";

// Contain named color getter method – the one which is used to define arranger BG colors
// It accepts string and returns RAW_COLOR class, easy to identify
const NAMED_COLOR_GETTER_1_ANCHOR: &str = "Remove all user input simulations";

// Other color anchor
// const OTHER_ANCHOR: &str = "Loop Region Fill";
// const OTHER_ANCHOR_2: &str = "Cue Marker Selected Fill";

// Used to search for raw color class, it has constants and one of them (black) is used for timeline playing position
const RAW_COLOR_ANCHOR: f64 = 0.666333;

/// Types of files that are useful for color theme extraction
#[derive(Debug, Clone)]
pub enum UsefulFileType {
    MainPalette,
    RawColor,
    Init,
    CrashReport,
    NamedColorGetter1,
    TimelineColorCnst {
        field_type_cp_idx: u16,
        fmim_idx: u16,
        cnst_name: String,
    },
}

/// Determine if a class file is useful for color theme processing
pub fn is_useful_file(class: &classfile::parse::Class) -> Option<UsefulFileType> {
    if let Some(mtch) = has_any_string_in_constant_pool(
        class,
        &[
            PALETTE_ANCHOR,
            INIT_ANCHOR,
            CRASH_REPORT_ANCHOR,
            NAMED_COLOR_GETTER_1_ANCHOR,
        ],
    ) {
        let useful_file_type = match mtch {
            PALETTE_ANCHOR => UsefulFileType::MainPalette,
            INIT_ANCHOR => UsefulFileType::Init,
            CRASH_REPORT_ANCHOR => UsefulFileType::CrashReport,
            NAMED_COLOR_GETTER_1_ANCHOR => UsefulFileType::NamedColorGetter1,
            _ => unreachable!(),
        };
        return Some(useful_file_type);
    }

    if has_any_double_in_constant_pool(class, &[RAW_COLOR_ANCHOR]).is_some() {
        return Some(UsefulFileType::RawColor);
    }

    if let Some((field_type_cp_idx, fmim_idx, cnst_name)) = detect_timeline_color_const(class) {
        return Some(UsefulFileType::TimelineColorCnst {
            field_type_cp_idx,
            fmim_idx,
            cnst_name,
        });
    }

    None
}

/// Check if any of the given strings exist in the class's constant pool
pub fn has_any_string_in_constant_pool<'a>(
    class: &classfile::parse::Class,
    strings: &[&'a str],
) -> Option<&'a str> {
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

/// Check if any of the given doubles exist in the class's constant pool
pub fn has_any_double_in_constant_pool(
    class: &classfile::parse::Class,
    doubles: &[f64],
) -> Option<f64> {
    for entry in &class.cp.0 {
        if let classfile::cpool::Const::Double(double_as_u64) = entry {
            let bytes = double_as_u64.to_be_bytes();
            let double_as_f64 = f64::from_be_bytes(bytes);
            if let Some(found) = doubles.iter().find(|dbl| **dbl == double_as_f64) {
                return Some(*found);
            }
        }
    }

    None
}

/// Detect timeline color constants in a class
pub fn detect_timeline_color_const(class: &classfile::parse::Class) -> Option<(u16, u16, String)> {
    use crate::jar::core::assembly::init_refprinter;
    use krakatau2::lib::disassemble::refprinter::ConstData;

    let rp = init_refprinter(&class.cp, &class.attrs);

    let method = class.methods.iter().find_map(|method| {
        let ConstData::Utf8(id) = &rp.cpool.get(method.desc as usize)?.data else {
            return None;
        };
        let sig = id.s.to_string();
        let sig_is_good = sig.starts_with("(Lcom/bitwig/graphics/") && sig.ends_with(";D)V");

        if sig_is_good {
            Some(method)
        } else {
            None
        }
    })?;

    let attr = method.attrs.first()?;
    let krakatau2::lib::classfile::attrs::AttrBody::Code((code_1, _)) = &attr.body else {
        return None;
    };

    let bytecode = &code_1.bytecode;
    for (_, ix) in &bytecode.0 {
        use krakatau2::lib::classfile::code::Instr;
        if let Instr::Putstatic(idx) = ix {
            let data = rp.cpool.get(*idx as usize)?;
            let ConstData::Fmim(
                krakatau2::lib::disassemble::refprinter::FmimTag::Field,
                field_type_cp_idx,
                fmim_idx,
            ) = data.data
            else {
                continue;
            };

            let fmim_data = rp.cpool.get(fmim_idx as usize)?;
            let ConstData::Nat(const_name_idx, _) = fmim_data.data else {
                continue;
            };

            let const_name_data = rp.cpool.get(const_name_idx as usize)?;
            let ConstData::Utf8(const_name) = &const_name_data.data else {
                continue;
            };

            return Some((field_type_cp_idx, *idx, const_name.s.to_string()));
        }
    }

    None
}

/// Extract release metadata from a class's constant pool
pub fn extract_release_metadata(class: &classfile::parse::Class) -> Option<Vec<(String, String)>> {
    // Find any strings in constant pool which contain the ": " substring
    let mut metadata = Vec::new();
    for entry in &class.cp.0 {
        if let classfile::cpool::Const::Utf8(txt) = entry {
            let parsed_string = String::from_utf8_lossy(txt.0);
            let Some((key, value)) = parsed_string.split_once(": ") else {
                continue;
            };
            let key = key.trim();
            let value = value.trim();
            let key_count = key.chars().filter(|c| c.is_alphanumeric()).count();
            let value_count = value.chars().filter(|c| c.is_alphanumeric()).count();
            if value_count == 0 || key_count == 0 || key == "Not obfuscated" {
                continue;
            }
            metadata.push((key.to_string(), value.to_string()));
        }
    }
    if metadata.is_empty() {
        None
    } else {
        Some(metadata)
    }
}
