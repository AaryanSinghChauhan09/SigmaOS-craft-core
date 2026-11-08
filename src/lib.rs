//! Shared project metadata and format-plugin contracts for SigmaOS Craft.
#![forbid(unsafe_code)]

use std::collections::{BTreeMap, BTreeSet};
use std::fmt;

/// Current version for the stable, format-independent project model.
pub const PROJECT_FORMAT_VERSION: u32 = 1;

/// A reference to an asset stored relative to a project root.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProjectAsset {
    id: String,
    relative_path: String,
    media_type: String,
    byte_len: u64,
}

impl ProjectAsset {
    /// Construct an asset reference after validating its ID, path, and media type.
    pub fn new(
        id: impl Into<String>,
        relative_path: impl Into<String>,
        media_type: impl Into<String>,
        byte_len: u64,
    ) -> Result<Self, ProjectError> {
        let asset = Self {
            id: id.into(),
            relative_path: relative_path.into(),
            media_type: media_type.into(),
            byte_len,
        };
        asset.validate()?;
        Ok(asset)
    }

    pub fn id(&self) -> &str {
        &self.id
    }
    pub fn relative_path(&self) -> &str {
        &self.relative_path
    }
    pub fn media_type(&self) -> &str {
        &self.media_type
    }
    pub fn byte_len(&self) -> u64 {
        self.byte_len
    }

    fn validate(&self) -> Result<(), ProjectError> {
        if self.id.is_empty()
            || !self
                .id
                .chars()
                .all(|ch| ch.is_ascii_alphanumeric() || "._-".contains(ch))
        {
            return Err(ProjectError::InvalidAssetId(self.id.clone()));
        }
        validate_relative_path(&self.relative_path)?;
        if self.media_type.trim().is_empty() || self.media_type.chars().any(char::is_whitespace) {
            return Err(ProjectError::EmptyField("asset media type"));
        }
        Ok(())
    }
}

/// A format-independent project document.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProjectManifest {
    format_version: u32,
    id: String,
    title: String,
    application_id: String,
    assets: Vec<ProjectAsset>,
}

impl ProjectManifest {
    /// Create a new project using the current schema version.
    pub fn new(
        id: impl Into<String>,
        title: impl Into<String>,
        application_id: impl Into<String>,
    ) -> Result<Self, ProjectError> {
        let manifest = Self {
            format_version: PROJECT_FORMAT_VERSION,
            id: id.into(),
            title: title.into(),
            application_id: application_id.into(),
            assets: Vec::new(),
        };
        manifest.validate()?;
        Ok(manifest)
    }

    pub fn format_version(&self) -> u32 {
        self.format_version
    }
    pub fn id(&self) -> &str {
        &self.id
    }
    pub fn title(&self) -> &str {
        &self.title
    }
    pub fn application_id(&self) -> &str {
        &self.application_id
    }
    pub fn assets(&self) -> &[ProjectAsset] {
        &self.assets
    }

    /// Add an asset, rejecting duplicate IDs and paths before changing the model.
    pub fn add_asset(&mut self, asset: ProjectAsset) -> Result<(), ProjectError> {
        asset.validate()?;
        if self.assets.iter().any(|existing| existing.id == asset.id) {
            return Err(ProjectError::DuplicateAssetId(asset.id));
        }
        if self
            .assets
            .iter()
            .any(|existing| existing.relative_path == asset.relative_path)
        {
            return Err(ProjectError::DuplicateAssetPath(asset.relative_path));
        }
        self.assets.push(asset);
        Ok(())
    }

    /// Check all externally supplied fields and collection invariants.
    pub fn validate(&self) -> Result<(), ProjectError> {
        if self.format_version != PROJECT_FORMAT_VERSION {
            return Err(ProjectError::UnsupportedVersion(self.format_version));
        }
        require_nonempty(&self.id, "project ID")?;
        require_nonempty(&self.title, "project title")?;
        require_nonempty(&self.application_id, "application ID")?;
        let mut ids = BTreeSet::new();
        let mut paths = BTreeSet::new();
        for asset in &self.assets {
            asset.validate()?;
            if !ids.insert(asset.id.as_str()) {
                return Err(ProjectError::DuplicateAssetId(asset.id.clone()));
            }
            if !paths.insert(asset.relative_path.as_str()) {
                return Err(ProjectError::DuplicateAssetPath(
                    asset.relative_path.clone(),
                ));
            }
        }
        Ok(())
    }
}

fn require_nonempty(value: &str, field: &'static str) -> Result<(), ProjectError> {
    if value.trim().is_empty() {
        Err(ProjectError::EmptyField(field))
    } else {
        Ok(())
    }
}

fn validate_relative_path(path: &str) -> Result<(), ProjectError> {
    if path.is_empty()
        || path.starts_with('/')
        || path.contains('\\')
        || path
            .split('/')
            .any(|part| part.is_empty() || part == "." || part == "..")
        || path
            .split('/')
            .next()
            .map_or(false, |part| part.contains(':'))
    {
        return Err(ProjectError::UnsafeAssetPath(path.to_owned()));
    }
    Ok(())
}

/// A project format adapter. Implementations own format-specific parsing only.
pub trait ProjectFormatPlugin: Send + Sync {
    fn id(&self) -> &str;
    fn extensions(&self) -> &[&str];
    fn decode(&self, bytes: &[u8]) -> Result<ProjectManifest, String>;
    fn encode(&self, manifest: &ProjectManifest) -> Result<Vec<u8>, String>;
}

/// Registry that composes format adapters without coupling the core model to a file format.
#[derive(Default)]
pub struct ProjectFormatRegistry {
    plugins: BTreeMap<String, Box<dyn ProjectFormatPlugin>>,
    extensions: BTreeMap<String, String>,
}

impl ProjectFormatRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    /// Register a plugin atomically; invalid or duplicate extensions add nothing.
    pub fn register(&mut self, plugin: Box<dyn ProjectFormatPlugin>) -> Result<(), ProjectError> {
        let id = plugin.id().trim().to_owned();
        if id.is_empty()
            || !id
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || b"._-".contains(&byte))
        {
            return Err(ProjectError::InvalidPluginId);
        }
        if self.plugins.contains_key(&id) {
            return Err(ProjectError::DuplicatePluginId(id));
        }

        let mut extensions = BTreeSet::new();
        for raw_extension in plugin.extensions() {
            let extension = normalize_extension(raw_extension)?;
            if self.extensions.contains_key(&extension) || !extensions.insert(extension.clone()) {
                return Err(ProjectError::DuplicateExtension(extension));
            }
        }
        if extensions.is_empty() {
            return Err(ProjectError::InvalidExtension(String::new()));
        }

        for extension in extensions {
            self.extensions.insert(extension, id.clone());
        }
        self.plugins.insert(id, plugin);
        Ok(())
    }

    /// Decode input with the plugin registered for `extension`, then validate its model.
    pub fn import(&self, extension: &str, bytes: &[u8]) -> Result<ProjectManifest, ProjectError> {
        let extension = normalize_extension(extension)?;
        let plugin_id = self
            .extensions
            .get(&extension)
            .ok_or_else(|| ProjectError::UnknownExtension(extension.clone()))?;
        let plugin = self
            .plugins
            .get(plugin_id)
            .ok_or_else(|| ProjectError::UnknownPlugin(plugin_id.clone()))?;
        let manifest = plugin.decode(bytes).map_err(ProjectError::Plugin)?;
        manifest.validate()?;
        Ok(manifest)
    }

    /// Validate and encode a project through a named format plugin.
    pub fn export(
        &self,
        plugin_id: &str,
        manifest: &ProjectManifest,
    ) -> Result<Vec<u8>, ProjectError> {
        manifest.validate()?;
        let plugin = self
            .plugins
            .get(plugin_id)
            .ok_or_else(|| ProjectError::UnknownPlugin(plugin_id.to_owned()))?;
        plugin.encode(manifest).map_err(ProjectError::Plugin)
    }

    pub fn plugin_count(&self) -> usize {
        self.plugins.len()
    }
}

fn normalize_extension(extension: &str) -> Result<String, ProjectError> {
    let extension = extension
        .trim()
        .trim_start_matches('.')
        .to_ascii_lowercase();
    if extension.is_empty() || !extension.bytes().all(|byte| byte.is_ascii_alphanumeric()) {
        return Err(ProjectError::InvalidExtension(extension));
    }
    Ok(extension)
}

/// Project model, format registry, or plugin failure.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProjectError {
    EmptyField(&'static str),
    UnsupportedVersion(u32),
    InvalidAssetId(String),
    UnsafeAssetPath(String),
    DuplicateAssetId(String),
    DuplicateAssetPath(String),
    InvalidPluginId,
    DuplicatePluginId(String),
    InvalidExtension(String),
    DuplicateExtension(String),
    UnknownExtension(String),
    UnknownPlugin(String),
    Plugin(String),
}

impl fmt::Display for ProjectError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::EmptyField(field) => write!(f, "{field} must not be empty"),
            Self::UnsupportedVersion(version) => {
                write!(f, "unsupported project format version: {version}")
            }
            Self::InvalidAssetId(id) => write!(f, "invalid asset ID: {id:?}"),
            Self::UnsafeAssetPath(path) => {
                write!(f, "asset path must be safe and project-relative: {path:?}")
            }
            Self::DuplicateAssetId(id) => write!(f, "duplicate asset ID: {id}"),
            Self::DuplicateAssetPath(path) => write!(f, "duplicate asset path: {path}"),
            Self::InvalidPluginId => write!(f, "plugin ID must not be empty"),
            Self::DuplicatePluginId(id) => write!(f, "duplicate plugin ID: {id}"),
            Self::InvalidExtension(extension) => {
                write!(f, "invalid project extension: {extension:?}")
            }
            Self::DuplicateExtension(extension) => {
                write!(f, "extension is already registered: {extension}")
            }
            Self::UnknownExtension(extension) => {
                write!(f, "no plugin handles extension: {extension}")
            }
            Self::UnknownPlugin(id) => write!(f, "unknown project format plugin: {id}"),
            Self::Plugin(error) => write!(f, "project format plugin failed: {error}"),
        }
    }
}

impl std::error::Error for ProjectError {}

/// Configuration contract for the Craft core component.
#[derive(Debug, Clone, PartialEq)]
pub struct Config {
    pub enabled: bool,
    pub component_name: String,
    pub tier: u8,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            enabled: true,
            component_name: String::from("SigmaOS-craft-core"),
            tier: 4,
        }
    }
}

/// Lifecycle state for the Craft core component.
#[derive(Debug, Clone, PartialEq)]
pub struct State {
    pub initialized: bool,
    pub healthy: bool,
}

/// Small component lifecycle adapter retained for existing callers.
pub struct Supervisor {
    config: Config,
    state: State,
}

impl Supervisor {
    pub fn new(config: Config) -> Self {
        Self {
            config,
            state: State {
                initialized: false,
                healthy: true,
            },
        }
    }

    pub fn initialize(&mut self) -> Result<String, String> {
        if !self.config.enabled {
            return Err(String::from("Component is administratively disabled"));
        }
        self.state.initialized = true;
        Ok(format!(
            "{} initialized successfully at tier {}",
            self.config.component_name, self.config.tier
        ))
    }

    pub fn is_healthy(&self) -> bool {
        self.state.initialized && self.state.healthy
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct TestFormat;

    impl ProjectFormatPlugin for TestFormat {
        fn id(&self) -> &str {
            "test-format"
        }
        fn extensions(&self) -> &[&str] {
            &["crafttest"]
        }
        fn decode(&self, _bytes: &[u8]) -> Result<ProjectManifest, String> {
            ProjectManifest::new("project-1", "Drawing", "sigmaos.craft.draw")
                .map_err(|error| error.to_string())
        }
        fn encode(&self, manifest: &ProjectManifest) -> Result<Vec<u8>, String> {
            Ok(manifest.id().as_bytes().to_vec())
        }
    }

    struct DuplicateExtensionFormat;

    impl ProjectFormatPlugin for DuplicateExtensionFormat {
        fn id(&self) -> &str {
            "second-format"
        }
        fn extensions(&self) -> &[&str] {
            &["crafttest"]
        }
        fn decode(&self, _bytes: &[u8]) -> Result<ProjectManifest, String> {
            Err("unused test decoder".into())
        }
        fn encode(&self, _manifest: &ProjectManifest) -> Result<Vec<u8>, String> {
            Err("unused test encoder".into())
        }
    }

    fn manifest() -> ProjectManifest {
        ProjectManifest::new("project-1", "Drawing", "sigmaos.craft.draw").unwrap()
    }

    #[test]
    fn asset_paths_reject_parent_and_absolute_paths() {
        let traversal = ProjectAsset::new("asset-1", "../secret.png", "image/png", 10);
        let absolute = ProjectAsset::new("asset-2", "/tmp/secret.png", "image/png", 10);

        assert!(matches!(traversal, Err(ProjectError::UnsafeAssetPath(_))));
        assert!(matches!(absolute, Err(ProjectError::UnsafeAssetPath(_))));
    }

    #[test]
    fn manifest_rejects_duplicate_asset_ids_and_paths() {
        let mut project = manifest();
        let first = ProjectAsset::new("asset-1", "assets/image.png", "image/png", 10).unwrap();
        let same_id = ProjectAsset::new("asset-1", "assets/other.png", "image/png", 11).unwrap();
        project.add_asset(first).unwrap();

        assert_eq!(
            project.add_asset(same_id),
            Err(ProjectError::DuplicateAssetId("asset-1".into()))
        );
        assert_eq!(project.assets().len(), 1);
    }

    #[test]
    fn plugin_registry_imports_and_exports_validated_projects() {
        let mut registry = ProjectFormatRegistry::new();
        registry.register(Box::new(TestFormat)).unwrap();

        let imported = registry.import(".CRAFTTEST", b"payload").unwrap();
        let exported = registry.export("test-format", &imported).unwrap();

        assert_eq!(imported.title(), "Drawing");
        assert_eq!(exported, b"project-1");
        assert_eq!(registry.plugin_count(), 1);
    }

    #[test]
    fn duplicate_extension_is_rejected_without_partial_registration() {
        let mut registry = ProjectFormatRegistry::new();
        registry.register(Box::new(TestFormat)).unwrap();
        let result = registry.register(Box::new(DuplicateExtensionFormat));

        assert_eq!(
            result,
            Err(ProjectError::DuplicateExtension("crafttest".into()))
        );
        assert_eq!(registry.plugin_count(), 1);
    }

    #[test]
    fn supervisor_keeps_disabled_component_unhealthy() {
        let config = Config {
            enabled: false,
            component_name: "SigmaOS-craft-core".into(),
            tier: 4,
        };
        let mut supervisor = Supervisor::new(config);

        let result = supervisor.initialize();

        assert!(result.is_err());
        assert!(!supervisor.is_healthy());
    }
}
