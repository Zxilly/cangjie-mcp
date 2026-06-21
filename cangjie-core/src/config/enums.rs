use std::fmt;
use std::str::FromStr;

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum EmbeddingType {
    None,
    Local,
    #[serde(rename = "openai")]
    OpenAI,
}

impl EmbeddingType {
    pub fn is_enabled(self) -> bool {
        self != EmbeddingType::None
    }
}

impl fmt::Display for EmbeddingType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            EmbeddingType::None => write!(f, "none"),
            EmbeddingType::Local => write!(f, "local"),
            EmbeddingType::OpenAI => write!(f, "openai"),
        }
    }
}

impl FromStr for EmbeddingType {
    type Err = String;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "none" => Ok(Self::None),
            "local" => Ok(Self::Local),
            "openai" => Ok(Self::OpenAI),
            _ => Err(format!("unknown embedding type: {s}")),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum RerankType {
    None,
    Local,
    #[serde(rename = "openai")]
    OpenAI,
}

impl fmt::Display for RerankType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            RerankType::None => write!(f, "none"),
            RerankType::Local => write!(f, "local"),
            RerankType::OpenAI => write!(f, "openai"),
        }
    }
}

impl FromStr for RerankType {
    type Err = String;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "none" => Ok(Self::None),
            "local" => Ok(Self::Local),
            "openai" => Ok(Self::OpenAI),
            _ => Err(format!("unknown rerank type: {s}")),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum DocLang {
    Zh,
    En,
}

impl DocLang {
    pub fn source_dir_name(self) -> &'static str {
        match self {
            DocLang::Zh => "source_zh_cn",
            DocLang::En => "source_en",
        }
    }

    pub fn runtime_source_dir_name(self) -> &'static str {
        match self {
            DocLang::Zh => "libs/std",
            DocLang::En => "libs/std_en",
        }
    }

    pub fn stdx_source_dir_name(self) -> &'static str {
        match self {
            DocLang::Zh => "libs_stdx",
            DocLang::En => "libs_stdx_en",
        }
    }
}

impl fmt::Display for DocLang {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            DocLang::Zh => write!(f, "zh"),
            DocLang::En => write!(f, "en"),
        }
    }
}

impl FromStr for DocLang {
    type Err = String;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "zh" => Ok(Self::Zh),
            "en" => Ok(Self::En),
            _ => Err(format!("unknown doc lang: {s}")),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PrebuiltMode {
    Off,
    Auto,
    Version(String),
}

impl PrebuiltMode {
    pub fn is_prebuilt(&self) -> bool {
        !matches!(self, PrebuiltMode::Off)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_embedding_type_from_str() {
        assert_eq!(
            "none".parse::<EmbeddingType>().unwrap(),
            EmbeddingType::None
        );
        assert_eq!(
            "local".parse::<EmbeddingType>().unwrap(),
            EmbeddingType::Local
        );
        assert_eq!(
            "openai".parse::<EmbeddingType>().unwrap(),
            EmbeddingType::OpenAI
        );
        assert!("invalid".parse::<EmbeddingType>().is_err());
    }

    #[test]
    fn test_rerank_type_from_str() {
        assert_eq!("none".parse::<RerankType>().unwrap(), RerankType::None);
        assert_eq!("local".parse::<RerankType>().unwrap(), RerankType::Local);
        assert_eq!("openai".parse::<RerankType>().unwrap(), RerankType::OpenAI);
        assert!("invalid".parse::<RerankType>().is_err());
    }

    #[test]
    fn test_doc_lang_from_str() {
        assert_eq!("zh".parse::<DocLang>().unwrap(), DocLang::Zh);
        assert_eq!("en".parse::<DocLang>().unwrap(), DocLang::En);
        assert!("invalid".parse::<DocLang>().is_err());
    }
}
