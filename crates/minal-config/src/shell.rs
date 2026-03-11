//! Shell configuration.

use serde::Deserialize;

/// Shell program settings.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(default)]
pub struct ShellConfig {
    /// Path to the shell program.
    pub program: String,
    /// Arguments to pass to the shell.
    pub args: Vec<String>,
}

impl Default for ShellConfig {
    fn default() -> Self {
        let program = std::env::var("SHELL").unwrap_or_else(|_| "/bin/sh".to_string());
        Self {
            program,
            args: vec!["--login".to_string()],
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_has_program() {
        let shell = ShellConfig::default();
        assert!(!shell.program.is_empty());
    }

    #[test]
    fn test_custom() {
        let toml_str = r#"
program = "/bin/bash"
args = ["-l"]
"#;
        let shell: ShellConfig = toml::from_str(toml_str).unwrap();
        assert_eq!(shell.program, "/bin/bash");
        assert_eq!(shell.args, vec!["-l"]);
    }
}
