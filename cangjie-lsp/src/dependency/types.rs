use std::collections::HashMap;

use serde::Serialize;

#[derive(Debug, Clone, Serialize)]
pub struct Dependency {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub git: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tag: Option<String>,
    pub path: String, // file:// URI
}

#[derive(Debug, Clone, Serialize, Default)]
pub struct PackageRequires {
    pub package_option: HashMap<String, String>,
    pub path_option: Vec<String>,
}

#[derive(Debug, Clone, Default, Serialize)]
pub struct ModuleOption {
    pub name: String,
    pub requires: HashMap<String, Dependency>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub package_requires: Option<PackageRequires>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub java_requires: Option<Vec<String>>,
}
