use std::collections::BTreeMap;
use std::{fs::File, collections::HashMap};
use std::io::BufReader;

use xml::reader::{EventReader, XmlEvent};

const NON_COLORS: &[&str] = &[
    "DefaultBlendFactor",
    "IconBlendFactor",
    "ClipBlendFactor",
    "NoteBorderStandbyBlendFactor",
    "RetroDisplayBlendFactor",
    "CheckControlNotCheckedBlendFactor",
    "MixSurfaceAreaBlendFactor",
    "TextFrameSegmentBlendFactor",
    "VelocityEditorForegroundSelectedBlendFactor",
    "NoteDisabledSelectedBlendFactor",
    "/WarperTimeBarMarkerBackgroun",
    "MinVelocityNoteBlendFactor",
    "StripedBackgroundShadeFactor",
    "AutomationLaneHeaderAlpha",
    "AutomationLaneClipBodyAlpha",
    "NonEditableAutomationAlpha",
    "DisabledContextMenuIconAlpha",
];

type AbletonColorDefs = BTreeMap<String, (u8, u8, u8, u8)>;

pub fn parse_ask(filename: &str) -> std::io::Result<AbletonColorDefs> {
    let file = File::open(filename)?;
    let file = BufReader::new(file); // Buffering is important for performance

    let parser = EventReader::new(file);
    let mut depth = 0;

    let mut color_defs = BTreeMap::new();

    let mut color_name = String::new();
    let mut r: u8 = 0;
    let mut g: u8 = 0;
    let mut b: u8 = 0;
    let mut a: u8 = 0;

    for e in parser {
        let event = e.unwrap();
        match event {
            XmlEvent::StartElement { name, attributes, .. } => {
                depth += 1;
                match depth {
                    3 => {
                        if NON_COLORS.contains(&name.to_string().as_str()) { continue; }
                        if !color_name.is_empty() {
                            color_defs.insert(color_name, (r, g, b, a));
                        }
                        color_name = name.to_string();
                    },
                    4 => {
                        let attr = &attributes.first().unwrap().value;
                        let val = match attr.parse::<f32>() {
                            Ok(num) => num,
                            Err(err) => {
                                panic!("Can't parse Ableton color component {}: {}", attr, err);
                            }
                        };
                        let val = val.round() as u8;
                        match name.to_string().as_str() {
                            "R" => r = val,
                            "G" => g = val,
                            "B" => b = val,
                            "Alpha" => a = val,
                            _ => {},
                        }
                    }
                    _ => {}
                }
            }
            XmlEvent::EndElement { .. } => {
                depth -= 1;
            }
            _ => {},
        }
    }

    Ok(color_defs)
}
