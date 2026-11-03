//! `transform_analysis_build.go`: a package's classes in its build info.

use rotor_support::gopath;

use crate::Error;
use crate::build_info::{BuildClass, BuildDecorator, BuildInfo};
use crate::plan::ClassPlan;
use crate::project::Project;
use crate::state::{DeclarationIdentity, FLAMEWORK_VERSION};
use crate::transform::{class_name_from_internal_id, lua_file_name};
use crate::transform_state::TransformState;

/// `addPackageBuildClass`: records a package's class, with its identifiers, in the build info.
pub(crate) fn add_package_build_class(
	state: &mut TransformState<'_>,
	class: &ClassPlan,
	file_name: &str,
) -> Result<(), Error> {
	let output = state.project.translator.get_output_path(file_name);
	let relative = gopath::rel(&state.project.root_directory, &output)
		.map_err(|error| Error::msg(format!("resolve Flamework build class path: {error}")))?;
	let class_id = state.identifier(&DeclarationIdentity {
		internal_id: class.internal_id.clone(),
		declaration_name: class_name_from_internal_id(&class.internal_id),
		lua_file_name: lua_file_name(file_name),
		..DeclarationIdentity::default()
	})?;
	let mut decorators = Vec::with_capacity(class.decorators.len());
	for decorator in &class.decorators {
		let (package_prefix, is_package) = package_prefix_for_internal_id(state.project, &decorator.internal_id)?;
		let decorator_id = state.identifier(&DeclarationIdentity {
			internal_id: decorator.internal_id.clone(),
			declaration_name: class_name_from_internal_id(&decorator.internal_id),
			lua_file_name: lua_file_name(file_name),
			package_prefix,
			is_package,
		})?;
		decorators.push(BuildDecorator {
			name: decorator.name.clone(),
			internal_id: decorator_id,
		});
	}
	state.project.add_class(&BuildClass {
		file_path: gopath::to_slash(&relative).into_owned(),
		internal_id: class_id,
		decorators,
	})
}

fn package_prefix_for_internal_id(project: &Project, internal_id: &str) -> Result<(String, bool), Error> {
	let Some(separator) = internal_id.find(':').filter(|separator| *separator > 0) else {
		return Ok((String::new(), false));
	};
	let package_name = &internal_id[..separator];
	if package_name == project.package_name {
		return Ok((String::new(), false));
	}
	let path = gopath::join(&[
		&project.directory,
		"node_modules",
		&gopath::from_slash(package_name),
		"flamework.build",
	]);
	let build = BuildInfo::load(&path, FLAMEWORK_VERSION)
		.map_err(|error| error.wrap("load packaged Flamework identifier metadata"))?;
	Ok((
		build.snapshot().identifier_prefix().unwrap_or_default().to_owned(),
		true,
	))
}

/// `buildClassByInternalID`: a binary search, then a linear one (the classes need not be
/// sorted).
fn build_class_by_internal_id(classes: &[BuildClass], internal_id: &str) -> Option<BuildClass> {
	// Go's `sort.Search`, whose probe order picks among duplicates on unsorted input.
	let (mut low, mut high) = (0, classes.len());
	while low < high {
		let middle = low + (high - low) / 2;
		if classes[middle].internal_id.as_str() >= internal_id {
			high = middle;
		} else {
			low = middle + 1;
		}
	}
	let index = low;
	if let Some(class) = classes.get(index).filter(|class| class.internal_id == internal_id) {
		return Some(class.clone());
	}
	classes.iter().find(|class| class.internal_id == internal_id).cloned()
}

/// `rootBuildClassByInternalID`: discovery's root-only lookup of a persisted class, by its
/// persisted identifier when there is one.
pub(crate) fn root_build_class_by_internal_id(project: &Project, internal_id: &str) -> Option<BuildClass> {
	let info = &project.build_info;
	let internal_id = info
		.identifiers
		.values
		.get(internal_id)
		.map_or(internal_id, String::as_str);
	build_class_by_internal_id(info.classes.as_deref().unwrap_or_default(), internal_id)
}
