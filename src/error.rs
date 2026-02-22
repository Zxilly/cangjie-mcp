use thiserror::Error;

#[derive(Debug, Error)]
pub enum CangjieError {
    #[error("索引未就绪")]
    IndexNotReady,

    #[error("文档未找到: {topic}")]
    DocumentNotFound { topic: String },

    #[error("LSP 不可用: {reason}")]
    LspUnavailable { reason: String },

    #[error("远程服务器错误: {0}")]
    RemoteServerError(String),

    #[error("嵌入维度不匹配: 期望 {expected}, 实际 {actual}")]
    DimensionMismatch { expected: usize, actual: usize },

    #[error(transparent)]
    Io(#[from] std::io::Error),

    #[error(transparent)]
    Serialization(#[from] serde_json::Error),

    #[error(transparent)]
    Other(#[from] anyhow::Error),
}
