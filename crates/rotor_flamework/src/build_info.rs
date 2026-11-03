//! `build_info.go`: `flamework.build`, the persisted identifiers, string hashes and classes.

use std::collections::{BTreeMap, HashMap};

use crate::build_info_clone::{clone_build_class, clone_string_pointer};
use crate::build_info_codec::OrderedStringMap;
use crate::id::new_uuid_v4;
use crate::runtime_config::RuntimeConfig;
use crate::{Error, ErrorKind};

/// `BuildInfoSchemaVersion`
pub const BUILD_INFO_SCHEMA_VERSION: f64 = 1.0;

/// A Go `[]string` that remembers whether it is nil, which `json.Marshal` writes as `null`.
pub type GoStrings = Option<Vec<String>>;

/// A Go `*map[string][]string`.
pub type GoStringSlicesMap = Option<BTreeMap<String, GoStrings>>;

/// `BuildDecorator`
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct BuildDecorator {
	/// `name`
	pub name: String,
	/// `internalId`
	pub internal_id: String,
}

/// `BuildClass`
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct BuildClass {
	/// `filePath`
	pub file_path: String,
	/// `internalId`
	pub internal_id: String,
	/// `decorators`
	pub decorators: Vec<BuildDecorator>,
}

/// `BuildGlobs`
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct BuildGlobs {
	/// `paths`: each glob's translated matches.
	pub paths: GoStringSlicesMap,
	/// `origins`: each file's globs.
	pub origins: GoStringSlicesMap,
}

/// `BuildMetadata`
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct BuildMetadata {
	/// `config`
	pub config: Option<RuntimeConfig>,
	/// `globs`
	pub globs: Option<BuildGlobs>,
}

/// `BuildInfoSnapshot`
#[derive(Clone, Debug, Default, PartialEq)]
pub struct BuildInfoSnapshot {
	/// The file's path.
	pub path: String,
	/// The schema version.
	pub version: f64,
	/// The Flamework version.
	pub flamework_version: String,
	/// The identifier prefix.
	pub prefix: Option<String>,
	/// The salt.
	pub salt: Option<String>,
	/// The metadata.
	pub metadata: Option<BuildMetadata>,
	/// The string hashes; `None` when absent.
	pub string_hashes: Option<HashMap<String, String>>,
	/// The identifiers.
	pub identifiers: HashMap<String, String>,
	/// The classes; `None` when absent.
	pub classes: Option<Vec<BuildClass>>,
}

impl BuildInfoSnapshot {
	/// `IdentifierPrefix`
	#[must_use]
	pub fn identifier_prefix(&self) -> Option<&str> {
		self.prefix.as_deref()
	}
}

/// `BuildInfo`
#[derive(Clone, Debug)]
pub struct BuildInfo {
	pub(crate) path: String,
	pub(crate) version: f64,
	pub(crate) flamework_version: String,
	pub(crate) prefix: Option<String>,
	pub(crate) salt: Option<String>,
	pub(crate) metadata: Option<BuildMetadata>,
	pub(crate) string_hashes: Option<OrderedStringMap>,
	pub(crate) identifiers: OrderedStringMap,
	pub(crate) classes: Option<Vec<BuildClass>>,
	pub(crate) field_order: Vec<String>,
	pub(crate) extras: HashMap<String, Vec<u8>>,
	pub(crate) packages: Vec<Self>,
	pub(crate) reverse_identifiers: HashMap<String, String>,
}

impl BuildInfo {
	/// `NewBuildInfo`
	#[must_use]
	pub fn new(path: &str, flamework_version: &str) -> Self {
		Self {
			path: path.to_owned(),
			version: BUILD_INFO_SCHEMA_VERSION,
			flamework_version: flamework_version.to_owned(),
			prefix: None,
			salt: None,
			metadata: None,
			string_hashes: None,
			identifiers: OrderedStringMap::default(),
			classes: None,
			field_order: vec![
				"version".to_owned(),
				"flameworkVersion".to_owned(),
				"identifiers".to_owned(),
			],
			extras: HashMap::new(),
			packages: Vec::new(),
			reverse_identifiers: HashMap::new(),
		}
	}

	/// `LoadBuildInfo`: the build info at `path`, or a new one when the file does not exist.
	///
	/// # Errors
	///
	/// The file cannot be read or is not a valid build info.
	pub fn load(path: &str, flamework_version: &str) -> Result<Self, Error> {
		let data = match std::fs::read(path) {
			Ok(data) => data,
			Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
				return Ok(Self::new(path, flamework_version));
			}
			Err(error) => return Err(Error::io("open", path, &error).wrap("flamework: read build info")),
		};
		let mut info = Self::new(path, flamework_version);
		info.decode(&data)?;
		Ok(info)
	}

	/// `Path`
	#[must_use]
	pub fn path(&self) -> &str {
		&self.path
	}

	/// `FlameworkVersion`
	#[must_use]
	pub fn flamework_version(&self) -> &str {
		&self.flamework_version
	}

	/// `LatestID`: the next identifier number.
	#[must_use]
	pub const fn latest_id(&self) -> usize {
		self.identifiers.order.len() + 1
	}

	/// `Prefix`
	#[must_use]
	pub fn prefix(&self) -> Option<&str> {
		self.prefix.as_deref()
	}

	/// `Salt`
	#[must_use]
	pub fn salt(&self) -> Option<&str> {
		self.salt.as_deref()
	}

	/// `SetIdentifierPrefix`
	pub fn set_identifier_prefix(&mut self, prefix: Option<&str>) {
		self.prefix = clone_string_pointer(prefix);
		self.ensure_field("identifierPrefix");
	}

	/// `SetSalt`
	pub fn set_salt(&mut self, salt: Option<&str>) {
		self.salt = clone_string_pointer(salt);
		self.ensure_field("salt");
	}

	/// `AddIdentifier`
	///
	/// # Errors
	///
	/// `internal_id` already has an identifier, here or in a package.
	pub fn add_identifier(&mut self, internal_id: &str, id: &str) -> Result<(), Error> {
		if self.identifier(internal_id).is_some() {
			return Err(Error::kinded(
				ErrorKind::DuplicateIdentifier,
				&format!(": {internal_id}"),
			));
		}
		self.identifiers.set(internal_id, id);
		self.reverse_identifiers.insert(id.to_owned(), internal_id.to_owned());
		Ok(())
	}

	/// `StringHash`
	#[must_use]
	pub fn string_hash(&self, key: &str) -> Option<&str> {
		self.string_hashes.as_ref()?.values.get(key).map(String::as_str)
	}

	/// `SetStringHash`
	pub fn set_string_hash(&mut self, key: &str, value: &str) {
		if self.string_hashes.is_none() {
			self.string_hashes = Some(OrderedStringMap::default());
			self.ensure_field("stringHashes");
		}
		if let Some(hashes) = &mut self.string_hashes {
			hashes.set(key, value);
		}
	}

	/// `HashString`: the persisted UUID for `context:text`, or a new one.
	///
	/// # Errors
	///
	/// The system's randomness failed.
	pub fn hash_string(&mut self, text: &str, context: &str) -> Result<String, Error> {
		let key = format!("{context}:{text}");
		if let Some(value) = self.string_hash(&key) {
			return Ok(value.to_owned());
		}
		let value = new_uuid_v4().map_err(|error| error.wrap("flamework: hash string"))?;
		self.set_string_hash(&key, &value);
		Ok(value)
	}

	/// `Identifier`: the identifier of `internal_id`, here or in a package.
	#[must_use]
	pub fn identifier(&self, internal_id: &str) -> Option<String> {
		if let Some(id) = self.identifiers.values.get(internal_id) {
			return Some(id.clone());
		}
		self.packages.iter().find_map(|child| child.identifier(internal_id))
	}

	/// `InternalIdentifier`: the internal ID an identifier belongs to.
	#[must_use]
	pub fn internal_identifier(&self, id: &str) -> Option<String> {
		if let Some(internal_id) = self.reverse_identifiers.get(id) {
			return Some(internal_id.clone());
		}
		// Go bug kept: packages are searched by internal ID, not by identifier.
		self.packages.iter().find_map(|child| child.identifier(id))
	}

	/// `AddClass`
	///
	/// # Errors
	///
	/// The class is already recorded, here or in a package.
	pub fn add_class(&mut self, class: &BuildClass) -> Result<(), Error> {
		if self.class(&class.internal_id).is_some() {
			return Err(Error::kinded(
				ErrorKind::DuplicateBuildClass,
				&format!(": {}", class.internal_id),
			));
		}
		if self.classes.is_none() {
			self.classes = Some(Vec::new());
			self.ensure_field("classes");
		}
		if let Some(classes) = &mut self.classes {
			classes.push(clone_build_class(class));
		}
		Ok(())
	}

	/// `Class`
	#[must_use]
	pub fn class(&self, internal_id: &str) -> Option<BuildClass> {
		if let Some(classes) = &self.classes
			&& let Some(class) = classes.iter().find(|class| class.internal_id == internal_id)
		{
			return Some(clone_build_class(class));
		}
		self.packages.iter().find_map(|child| child.class(internal_id))
	}

	/// `AddPackage`
	pub fn add_package(&mut self, child: Self) {
		self.packages.push(child);
	}

	pub(crate) fn ensure_field(&mut self, name: &str) {
		if !self.field_order.iter().any(|field| field == name) {
			self.field_order.push(name.to_owned());
		}
	}
}
