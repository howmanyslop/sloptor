//! `solution_satisfied.go`: a satisfied project's restored outputs must exist and parse.

use rotor_support::gopath;
use rotor_tsrs::tscore::ScriptKind;

use crate::crossproject::project_restored_declarations;
use crate::solution::SolutionProject;
use crate::solution_cache::{SolutionCompileCache, declaration_source_file_parse_options};
use crate::solution_drain::{SolutionBuildDrainer, SolutionSelectionError};

impl SolutionBuildDrainer {
	/// `validateSatisfiedProject`
	pub(crate) fn validate_satisfied_project(
		&mut self,
		project: &SolutionProject,
		cache: &SolutionCompileCache,
	) -> Option<SolutionSelectionError> {
		let invalid = |detail: &str| SolutionSelectionError {
			config_path: project.config_path.clone(),
			code: "SOLUTION_SATISFIED_OUTPUT_INVALID",
			message: format!(
				"satisfied project {} has invalid restored metadata: {detail}",
				project.config_path
			),
		};
		if let Some(detail) = self.restored_metadata_errs.get(&project.config_path)
			&& !detail.is_empty()
		{
			return Some(invalid(detail));
		}
		if !self.restored_declarations.contains_key(&project.config_path) {
			match project_restored_declarations(project) {
				Ok(declarations) => {
					self.restored_declarations
						.insert(project.config_path.clone(), declarations);
				}
				Err(error) => return Some(invalid(&error.to_string())),
			}
		}
		let declarations = self.restored_declarations.get(&project.config_path)?;
		for declaration_path in declarations {
			let shown = gopath::clean(&gopath::from_slash(declaration_path));
			let Some(declaration_text) = SolutionCompileCache::read_file(declaration_path) else {
				return Some(SolutionSelectionError {
					config_path: project.config_path.clone(),
					code: "SOLUTION_SATISFIED_OUTPUT_MISSING",
					message: format!(
						"satisfied project {} is missing restored declaration output {shown}; restore the output or select the project for work",
						project.config_path
					),
				});
			};
			let parse_options = declaration_source_file_parse_options(declaration_path);
			let parsed = cache.get_source_file(&parse_options, |options| {
				Some(rotor_tsrs::parser::parse_source_file(
					options,
					&declaration_text,
					ScriptKind::TS,
				))
			});
			let detail = parsed.map_or_else(
				|| Some("the declaration could not be parsed".to_owned()),
				|parsed| parsed.diagnostics().first().map(|diagnostic| diagnostic.localize()),
			);
			if let Some(detail) = detail {
				return Some(SolutionSelectionError {
					config_path: project.config_path.clone(),
					code: "SOLUTION_SATISFIED_OUTPUT_INVALID",
					message: format!(
						"satisfied project {} has invalid restored declaration output {shown}: {detail}; restore the output or select the project for work",
						project.config_path
					),
				});
			}
		}
		None
	}
}
