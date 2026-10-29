use zed_extension_api::{self as zed, settings::LspSettings};

const LANGUAGE_SERVER_ID: &str = "sloptor";

struct SloptorExtension;

impl zed::Extension for SloptorExtension {
	fn new() -> Self {
		Self
	}

	fn language_server_command(
		&mut self,
		language_server_id: &zed::LanguageServerId,
		worktree: &zed::Worktree,
	) -> zed::Result<zed::Command> {
		let binary = LspSettings::for_worktree(language_server_id.as_ref(), worktree)?.binary;
		let path = binary.as_ref().and_then(|binary| binary.path.clone());
		let arguments = binary.and_then(|binary| binary.arguments);

		let command = path.or_else(|| worktree.which(LANGUAGE_SERVER_ID)).ok_or_else(|| {
			"sloptor was not found. Put it on your PATH or set `lsp.sloptor.binary.path` in Zed's settings.".to_string()
		})?;

		Ok(zed::Command {
			command,
			args: arguments.unwrap_or_else(|| vec!["lsp".to_string(), "--stdio".to_string()]),
			env: worktree.shell_env(),
		})
	}

	fn language_server_initialization_options(
		&mut self,
		language_server_id: &zed::LanguageServerId,
		worktree: &zed::Worktree,
	) -> zed::Result<Option<zed::serde_json::Value>> {
		Ok(LspSettings::for_worktree(language_server_id.as_ref(), worktree)?.initialization_options)
	}
}

zed::register_extension!(SloptorExtension);
