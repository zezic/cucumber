use std::collections::HashMap;

use serde::Deserialize;

#[derive(Debug, Deserialize)]
pub struct BerikaiTheme {
    pub window: HashMap<String, String>,
    pub arranger: HashMap<String, String>,
}
