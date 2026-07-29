use glob::glob;
use serde::Deserialize;
use serde::Serialize;
use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::io;
use std::path::Path;
use std::path::PathBuf;

#[allow(unused_imports)]
use tracing::{Level, debug, error, info, trace, warn};

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub struct TomlManifest {
    pub dependencies: Option<BTreeMap<String, toml::Value>>,
    pub dev_dependencies: Option<BTreeMap<String, toml::Value>>,
    pub build_dependencies: Option<BTreeMap<String, toml::Value>>,
    pub target: Option<BTreeMap<String, toml::Value>>,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub struct WorkspaceTable {
    pub members: Option<Vec<PathBuf>>,
    pub exclude: Option<BTreeSet<PathBuf>>,
    pub default_members: Option<Vec<PathBuf>>,
    #[serde(flatten)]
    pub extra: Option<TomlManifest>,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub struct WorkspaceTomlManifest {
    pub workspace: WorkspaceTable,
}

pub fn is_workspace(src: &Path) -> io::Result<bool> {
    if let Ok(manifest) = fs::read_to_string(src) {
        if let Ok(manifest_data) = toml::from_str::<toml::Value>(&manifest) {
            Ok(manifest_data.get("workspace").is_some())
        } else {
            Ok(false)
        }
    } else {
        Err(io::Error::other(format!(
            "Failed to check manifest file at path {}",
            src.to_string_lossy()
        )))
    }
}

pub fn workspace_has_dependencies(workdir: &Path, src: &Path) -> io::Result<bool> {
    let mut global_has_deps = false;
    let src_parent = src.parent().unwrap_or(workdir);
    if let Ok(manifest) = fs::read_to_string(src) {
        match toml::from_str::<WorkspaceTomlManifest>(&manifest) {
            Ok(manifest_data) => {
                if let Some(extra_data) = manifest_data.workspace.extra {
                    global_has_deps = match extra_data.dependencies {
                        Some(deps) => !deps.is_empty(),
                        None => false,
                    } || match extra_data.dev_dependencies {
                        Some(deps) => !deps.is_empty(),
                        None => false,
                    } || match extra_data.build_dependencies {
                        Some(deps) => !deps.is_empty(),
                        None => false,
                    } || match extra_data.target {
                        Some(deps) => !deps.is_empty(),
                        None => false,
                    }
                }
                let mut members_paths: Vec<PathBuf> = Vec::new();
                if let Some(mut members) = manifest_data.workspace.members {
                    members_paths.append(&mut members);
                };

                if let Some(mut members) = manifest_data.workspace.default_members {
                    members_paths.append(&mut members);
                }

                let exclude_paths: BTreeSet<_> =
                    manifest_data.workspace.exclude.unwrap_or_default();

                let exclude_paths: BTreeSet<_> = exclude_paths
                    .into_iter()
                    .map(|exclude_path| src_parent.join(exclude_path))
                    .collect();

                members_paths.sort();
                members_paths.dedup();
                debug!(?members_paths);
                for member in members_paths {
                    if *member.to_string_lossy() == *"." {
                        warn!("⚠️ Workspace has membered itself at the root of the project.");
                        global_has_deps = true;
                        continue;
                    }

                    let member_path = src_parent.join(member);
                    let mut member_glob_paths: Vec<PathBuf> = glob(&member_path.to_string_lossy())
                        .map_err(|err| {
                            error!(?err);
                            io::Error::new(io::ErrorKind::NotFound, "Glob pattern not found")
                        })?
                        .flatten()
                        .collect();

                    debug!(?member_glob_paths);
                    while let Some(glob_member_path) = member_glob_paths.pop() {
                        debug!(?glob_member_path);
                        let member_path_from_glob = src_parent.join(glob_member_path);
                        debug!(?member_path_from_glob);

                        if exclude_paths.contains(&member_path_from_glob) {
                            debug!(
                                ?member_path_from_glob,
                                "Excluding path from member consideration"
                            );
                            continue;
                        }

                        if member_path_from_glob.is_dir() {
                            let possible_manifest_at_path =
                                member_path_from_glob.join("Cargo.toml");
                            if possible_manifest_at_path.is_file() {
                                debug!(?possible_manifest_at_path, "🐈 Found a membered path.");
                                let is_workspace = is_workspace(&possible_manifest_at_path)?;
                                if is_workspace {
                                    global_has_deps = global_has_deps
                                        || workspace_has_dependencies(
                                            workdir,
                                            &possible_manifest_at_path,
                                        )?;
                                } else {
                                    global_has_deps = global_has_deps
                                        || has_dependencies(&possible_manifest_at_path)?;
                                }
                            } else {
                                let msg = "The member path does not seem to be a file.";
                                error!(?possible_manifest_at_path, msg);
                                return Err(io::Error::new(io::ErrorKind::NotFound, msg));
                            }
                        } else if member_path_from_glob.is_file()
                            && let Some(filename) = member_path_from_glob.file_name()
                        {
                            let filename = filename.to_string_lossy();
                            if filename == *"Cargo.toml" {
                                debug!(?member_path_from_glob, "🐈 Found a membered path.");
                                let is_workspace = is_workspace(&member_path_from_glob)?;
                                if is_workspace {
                                    global_has_deps = global_has_deps
                                        || workspace_has_dependencies(
                                            workdir,
                                            &member_path_from_glob,
                                        )?;
                                } else {
                                    global_has_deps = global_has_deps
                                        || has_dependencies(&member_path_from_glob)?;
                                }
                            }
                        }
                    }
                }
            }
            Err(err) => {
                error!(?err, "Failed to deserialize TOML manifest file.");
                return Err(io::Error::new(io::ErrorKind::InvalidData, err.to_string()));
            }
        };
        return Ok(global_has_deps);
    };

    Err(io::Error::other(format!(
        "Failed to check manifest file at path {}",
        src.to_string_lossy()
    )))
}

pub fn has_dependencies(src: &Path) -> io::Result<bool> {
    if let Ok(manifest) = fs::read_to_string(src) {
        match toml::from_str::<TomlManifest>(&manifest) {
            Ok(manifest_data) => {
                debug!(?manifest_data, "Manifest TOML data");
                return Ok(match manifest_data.dependencies {
                    Some(deps) => !deps.is_empty(),
                    None => false,
                } || match manifest_data.dev_dependencies {
                    Some(deps) => !deps.is_empty(),
                    None => false,
                } || match manifest_data.build_dependencies {
                    Some(deps) => !deps.is_empty(),
                    None => false,
                } || match manifest_data.target {
                    Some(deps) => !deps.is_empty(),
                    None => false,
                });
            }
            Err(err) => {
                debug!(?err, "Failed to deserialize toml.");
            }
        };
    };
    Err(io::Error::other(format!(
        "Failed to check manifest file at path {}",
        src.to_string_lossy()
    )))
}
