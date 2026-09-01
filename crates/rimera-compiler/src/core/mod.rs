use std::fmt;
use std::path::PathBuf;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct Span {
    pub start: u32,
    pub end: u32,
}

impl Span {
    #[must_use]
    pub const fn new(start: u32, end: u32) -> Self {
        Self { start, end }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Diagnostic {
    pub code: &'static str,
    pub message: String,
    pub path: PathBuf,
    pub span: Span,
}

impl Diagnostic {
    #[must_use]
    pub fn new(
        code: &'static str,
        message: impl Into<String>,
        path: impl Into<PathBuf>,
        span: Span,
    ) -> Self {
        Self {
            code,
            message: message.into(),
            path: path.into(),
            span,
        }
    }
}

impl fmt::Display for Diagnostic {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            formatter,
            "error[{}]: {}\n --> {}:{}..{}",
            self.code,
            self.message,
            self.path.display(),
            self.span.start,
            self.span.end
        )
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct DiagnosticSet(Vec<Diagnostic>);

impl DiagnosticSet {
    #[must_use]
    pub fn one(diagnostic: Diagnostic) -> Self {
        Self(vec![diagnostic])
    }

    #[must_use]
    pub fn as_slice(&self) -> &[Diagnostic] {
        &self.0
    }
}

impl fmt::Display for DiagnosticSet {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        for (index, diagnostic) in self.0.iter().enumerate() {
            if index > 0 {
                writeln!(formatter)?;
            }
            write!(formatter, "{diagnostic}")?;
        }
        Ok(())
    }
}

impl std::error::Error for DiagnosticSet {}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TargetTriple(String);

impl TargetTriple {
    pub const MACOS_ARM64: &'static str = "aarch64-apple-darwin";

    pub fn parse(value: impl Into<String>) -> Result<Self, String> {
        let value = value.into();
        if value == Self::MACOS_ARM64 {
            Ok(Self(value))
        } else {
            Err(format!(
                "unsupported target `{value}`; expected {}",
                Self::MACOS_ARM64
            ))
        }
    }

    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl Default for TargetTriple {
    fn default() -> Self {
        Self(Self::MACOS_ARM64.to_owned())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum BuildProfile {
    #[default]
    Debug,
    Release,
}
