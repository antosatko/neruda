use zed_extension_api::{self as zed, Result};

struct NerudaExtension;

impl zed::Extension for NerudaExtension {
    fn new() -> Self {
        Self
    }

    fn language_server_command(
        &mut self,
        config: &zed::LanguageServerId,
        worktree: &zed::Worktree,
    ) -> Result<zed::Command> {
        let path = worktree
            .which("neruda-lsp")
            .unwrap_or_else(|| "neruda-lsp".to_string());

        Ok(zed::Command {
            command: path,
            args: vec![],
            env: Default::default(),
        })
    }
}

zed::register_extension!(NerudaExtension);
