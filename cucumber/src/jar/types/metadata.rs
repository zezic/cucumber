use crate::jar::{
    analysis::NamedColorGetterInvocation,
    types::{
        colors::{NamedColor, RawColorConstants},
        methods::{MethodDescription, PaletteColorMethods, RawColorMethods},
    },
    utils::legacy::TimelineColorReference,
};

#[derive(Debug)]
pub struct GeneralGoodies {
    pub init_class: String,
    pub named_colors: Vec<NamedColor>,
    pub palette_color_methods: PaletteColorMethods,
    pub raw_colors: RawColorGoodies,
    pub timeline_color_ref: Option<TimelineColorReference>, // Don't exist on 5.2.4?
    pub release_metadata: Vec<(String, String)>,
    pub named_color_getter_1: MethodDescription,
    pub named_color_getter_invocations: Vec<(String, NamedColorGetterInvocation)>,
}

// Color methods and defined static colors (contain important black color)
#[derive(Debug)]
pub struct RawColorGoodies {
    #[allow(dead_code)]
    pub methods: RawColorMethods,
    pub constants: RawColorConstants,
}
