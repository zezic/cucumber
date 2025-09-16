use krakatau2::lib::classfile::{
    cpool::{BStr, Const},
    parse::Class,
};

#[derive(Debug)]
pub struct TimelineColorReference {
    pub class_filename: String,
    pub const_name: String,
    pub field_type_cp_idx: u16,
    pub fmim_idx: u16,
}

/// Was used on pre 5.x.something to switch timeline cursor color
#[allow(dead_code)]
fn switch_timeline_color<'a>(
    class: &mut Class<'a>,
    new_const: &'a str,
    timeline_color_ref: &mut TimelineColorReference,
) -> Option<()> {
    let utf_data_idx = class.cp.0.len();
    class.cp.0.push(Const::Utf8(BStr(new_const.as_bytes())));

    let nat_idx = class.cp.0.len();
    class.cp.0.push(Const::NameAndType(
        utf_data_idx as u16,
        timeline_color_ref.field_type_cp_idx,
    ));

    let Const::Field(_, old_nat_idx) = class.cp.0.get_mut(timeline_color_ref.fmim_idx as usize)?
    else {
        panic!()
    };
    *old_nat_idx = nat_idx as u16;

    timeline_color_ref.const_name = new_const.to_string();
    Some(())
}
