use std::fmt;

pub type CoreResult<T> = Result<T, CoreError>;

#[derive(Debug)]
pub enum CoreError {
    Message(String),
    Io(std::io::Error),
    Json(serde_json::Error),
    #[cfg(feature = "native-anyconnect")]
    AnyConnect(String),
}

impl CoreError {
    pub fn msg(message: impl Into<String>) -> Self {
        Self::Message(message.into())
    }
}

impl fmt::Display for CoreError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Message(message) => write!(f, "{message}"),
            Self::Io(err) => write!(f, "io: {err}"),
            Self::Json(err) => write!(f, "json: {err}"),
            #[cfg(feature = "native-anyconnect")]
            Self::AnyConnect(err) => write!(f, "anyconnect: {err}"),
        }
    }
}

impl std::error::Error for CoreError {}

impl From<std::io::Error> for CoreError {
    fn from(value: std::io::Error) -> Self {
        Self::Io(value)
    }
}

impl From<serde_json::Error> for CoreError {
    fn from(value: serde_json::Error) -> Self {
        Self::Json(value)
    }
}

#[cfg(feature = "native-anyconnect")]
impl From<anyconnect::Error> for CoreError {
    fn from(value: anyconnect::Error) -> Self {
        Self::AnyConnect(value.to_string())
    }
}
