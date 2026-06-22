use serde::{Deserialize, Serialize};

/// Describes a guild that the kernel can load.
/// This is the manifest format used by the SemanticRouter.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GuildDescriptor {
    pub name: String,
    pub description: String,
    pub guild_type: GuildType,
    /// For local guilds: relative path to the Python file
    pub path: Option<String>,
    /// For proxy guilds: command to spawn
    pub command: Option<String>,
    /// For proxy guilds: arguments to the command
    pub args: Option<Vec<String>>,
    /// For proxy guilds: environment variables
    pub env: Option<std::collections::HashMap<String, String>>,
    /// Pre-computed embedding vector (set at runtime)
    #[serde(skip)]
    pub embedding: Option<Vec<f32>>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum GuildType {
    Local,
    Proxy,
    Service,
}

/// Result of a tool execution, compatible with MCP protocol.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolResult {
    pub content: Vec<ContentItem>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub is_error: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContentItem {
    #[serde(rename = "type")]
    pub content_type: String,
    pub text: String,
}

impl ToolResult {
    pub fn text(msg: impl Into<String>) -> Self {
        Self {
            content: vec![ContentItem {
                content_type: "text".into(),
                text: msg.into(),
            }],
            is_error: None,
        }
    }

    pub fn error(msg: impl Into<String>) -> Self {
        Self {
            content: vec![ContentItem {
                content_type: "text".into(),
                text: msg.into(),
            }],
            is_error: Some(true),
        }
    }
}

/// Security channel classification.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Channel {
    /// Local standard input/output (IDE connection)
    Stdio,
    /// Remote HTTP request
    Http { authenticated: bool },
    /// Remote Server-Sent Events session
    Sse { authenticated: bool },
    /// Local Command Line Interface
    Cli,
    /// Internal local invocation
    Local,
    /// Unknown or third-party channel
    Unknown(String),
}

impl Channel {
    /// Determines if the channel is inherently trusted or has been authenticated.
    pub fn is_trusted(&self) -> bool {
        match self {
            Channel::Stdio | Channel::Cli | Channel::Local => true,
            Channel::Http { authenticated } => *authenticated,
            Channel::Sse { authenticated } => *authenticated,
            Channel::Unknown(_) => false,
        }
    }
}

impl std::fmt::Display for Channel {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Channel::Stdio => write!(f, "stdio"),
            Channel::Http { authenticated } => {
                write!(f, "http{}", if *authenticated { "(auth)" } else { "(anon)" })
            }
            Channel::Sse { authenticated } => {
                write!(f, "sse{}", if *authenticated { "(auth)" } else { "(anon)" })
            }
            Channel::Cli => write!(f, "cli"),
            Channel::Local => write!(f, "local"),
            Channel::Unknown(s) => write!(f, "unknown({})", s),
        }
    }
}
