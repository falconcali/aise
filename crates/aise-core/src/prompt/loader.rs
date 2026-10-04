use crate::prompt::manifest::{MANIFEST_FILE_NAME, PromptManifest, PromptManifestEntry};
use crate::prompt::{PromptConfig, PromptError, PromptLayer};
use std::collections::HashSet;
use std::fs::{self, File};
use std::io::{self, Read};
use std::path::{Component, Path, PathBuf};
use std::sync::Arc;

const NON_ZERO_REASON: &str = "must be non-zero";

pub(crate) struct LoadedPrompt {
    pub(crate) id: Arc<str>,
    pub(crate) csi: String,
    pub(crate) rc: String,
    pub(crate) fti: String,
}

impl LoadedPrompt {
    pub(crate) fn source(&self, layer: PromptLayer) -> &str {
        match layer {
            PromptLayer::Csi => &self.csi,
            PromptLayer::Rc => &self.rc,
            PromptLayer::Fti => &self.fti,
        }
    }
}

pub(crate) struct LoadedCatalog {
    pub(crate) prompts: Vec<LoadedPrompt>,
    pub(crate) total_template_bytes: u64,
}

pub(crate) fn load_catalog(config: &PromptConfig) -> Result<LoadedCatalog, PromptError> {
    validate_config(config)?;
    let manifest = read_manifest(config)?;
    if manifest.prompts.len() > config.max_prompts {
        return Err(PromptError::TooManyPrompts {
            count: manifest.prompts.len(),
            limit: config.max_prompts,
        });
    }
    let mut seen = HashSet::with_capacity(manifest.prompts.len());
    let mut prompts = Vec::with_capacity(manifest.prompts.len());
    let mut total_template_bytes = 0;
    for (index, entry) in manifest.prompts.iter().enumerate() {
        if entry.id.trim().is_empty() {
            return Err(PromptError::EmptyPromptId { index });
        }
        if !seen.insert(entry.id.as_str()) {
            return Err(PromptError::DuplicatePromptId {
                prompt_id: entry.id.clone(),
            });
        }
        prompts.push(load_prompt(config, entry, &mut total_template_bytes)?);
    }
    Ok(LoadedCatalog {
        prompts,
        total_template_bytes,
    })
}

pub(crate) fn validate_config(config: &PromptConfig) -> Result<(), PromptError> {
    let limits = [
        ("max_prompts", config.max_prompts),
        ("max_template_bytes", config.max_template_bytes),
        ("max_total_template_bytes", config.max_total_template_bytes),
    ];
    match limits.into_iter().find(|(_, value)| *value == 0) {
        Some((field, _)) => Err(PromptError::InvalidConfig {
            field,
            reason: NON_ZERO_REASON,
        }),
        None => Ok(()),
    }
}

pub(crate) fn resolve_template_path(
    root: &Path,
    prompt_id: &str,
    layer: PromptLayer,
    relative: &str,
) -> Result<PathBuf, PromptError> {
    let invalid = |reason| PromptError::InvalidTemplatePath {
        prompt_id: prompt_id.to_owned(),
        layer,
        path: PathBuf::from(relative),
        reason,
    };
    if relative.is_empty() {
        return Err(invalid("empty"));
    }
    let relative_path = Path::new(relative);
    if relative_path.is_absolute()
        || relative_path
            .components()
            .any(|component| matches!(component, Component::Prefix(_) | Component::RootDir))
    {
        return Err(invalid("absolute"));
    }
    if relative_path
        .components()
        .any(|component| matches!(component, Component::ParentDir))
    {
        return Err(invalid("parent_component"));
    }
    let canonical_root = canonicalize(root, prompt_id, layer)?;
    let canonical_path = canonicalize(&root.join(relative_path), prompt_id, layer)?;
    if !canonical_path.starts_with(&canonical_root) {
        return Err(invalid("outside_root"));
    }
    Ok(canonical_path)
}

fn canonicalize(path: &Path, prompt_id: &str, layer: PromptLayer) -> Result<PathBuf, PromptError> {
    path.canonicalize().map_err(|source| PromptError::TemplateRead {
        prompt_id: prompt_id.to_owned(),
        layer,
        path: path.to_path_buf(),
        source,
    })
}

fn read_manifest(config: &PromptConfig) -> Result<PromptManifest, PromptError> {
    let path = config.directory.join(MANIFEST_FILE_NAME);
    let read_error = |source| PromptError::ManifestRead {
        path: path.clone(),
        source,
    };
    let bytes = fs::metadata(&path).map_err(read_error)?.len();
    if bytes > config.max_template_bytes as u64 {
        return Err(PromptError::ManifestTooLarge {
            path,
            bytes,
            limit: config.max_template_bytes,
        });
    }
    let text = fs::read_to_string(&path).map_err(read_error)?;
    toml::from_str::<PromptManifest>(&text).map_err(|source| PromptError::ManifestParse {
        path,
        source: Box::new(source),
    })
}

fn load_prompt(
    config: &PromptConfig,
    entry: &PromptManifestEntry,
    total_template_bytes: &mut u64,
) -> Result<LoadedPrompt, PromptError> {
    let csi = load_layer(config, entry, PromptLayer::Csi, total_template_bytes)?;
    let rc = load_layer(config, entry, PromptLayer::Rc, total_template_bytes)?;
    let fti = load_layer(config, entry, PromptLayer::Fti, total_template_bytes)?;
    Ok(LoadedPrompt {
        id: Arc::from(entry.id.as_str()),
        csi,
        rc,
        fti,
    })
}

fn load_layer(
    config: &PromptConfig,
    entry: &PromptManifestEntry,
    layer: PromptLayer,
    total_template_bytes: &mut u64,
) -> Result<String, PromptError> {
    let path = resolve_template_path(&config.directory, &entry.id, layer, entry.path(layer))?;
    let source = read_template(config, &entry.id, layer, &path)?;
    *total_template_bytes += source.len() as u64;
    if *total_template_bytes > config.max_total_template_bytes as u64 {
        return Err(PromptError::TotalTemplateBytesExceeded {
            bytes: *total_template_bytes,
            limit: config.max_total_template_bytes,
        });
    }
    Ok(source)
}

fn read_template(
    config: &PromptConfig,
    prompt_id: &str,
    layer: PromptLayer,
    path: &Path,
) -> Result<String, PromptError> {
    let read_error = |source| PromptError::TemplateRead {
        prompt_id: prompt_id.to_owned(),
        layer,
        path: path.to_path_buf(),
        source,
    };
    let mut bytes = Vec::new();
    File::open(path)
        .map_err(read_error)?
        .take(config.max_template_bytes as u64 + 1)
        .read_to_end(&mut bytes)
        .map_err(read_error)?;
    if bytes.len() > config.max_template_bytes {
        return Err(PromptError::TemplateTooLarge {
            prompt_id: prompt_id.to_owned(),
            layer,
            path: path.to_path_buf(),
            bytes: bytes.len() as u64,
            limit: config.max_template_bytes,
        });
    }
    String::from_utf8(bytes).map_err(|error| read_error(io::Error::new(io::ErrorKind::InvalidData, error)))
}

#[cfg(test)]
#[path = "tests/loader_tests.rs"]
mod tests;
