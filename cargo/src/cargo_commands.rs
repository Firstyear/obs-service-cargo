use std::fs;
use std::io;
use std::path::Path;
use std::path::PathBuf;

#[allow(unused_imports)]
use tracing::{Level, debug, error, info, trace, warn};

use crate::audit;
// use crate::target::TARGET_TRIPLES;
use crate::toml_manifest::has_dependencies;
use crate::toml_manifest::is_workspace;
use crate::toml_manifest::workspace_has_dependencies;

fn cargo_command(
    subcommand: &str,
    options: &[String],
    curdir: impl AsRef<Path>,
) -> io::Result<String> {
    let cmd = std::process::Command::new("cargo")
        .arg(subcommand)
        .args(options.iter())
        .current_dir(curdir.as_ref())
        .output()?;
    trace!(?cmd);
    let stdoutput = String::from_utf8_lossy(&cmd.stdout);
    let stderrput = String::from_utf8_lossy(&cmd.stderr);
    if !cmd.status.success() {
        debug!(?stdoutput);
        debug!(?stderrput);
        return Err(io::Error::new(io::ErrorKind::Interrupted, stderrput));
    };
    debug!(?stdoutput);
    debug!(?stderrput);
    // Return the output on success as this has the infor for .cargo/config
    Ok(stdoutput.to_string())
}

pub fn cargo_fetch(curdir: &Path, manifest: &str, respect_lockfile: bool) -> io::Result<String> {
    info!("🚅 Running `cargo fetch`...");
    let mut default_options: Vec<String> = vec![];
    let manifest_path = PathBuf::from(&manifest).canonicalize()?;
    if !manifest_path.is_file() {
        let msg = format!(
            "🛑 There seems to be no manifest at this path `{}`.",
            manifest_path.display()
        );
        error!(msg, ?manifest_path);
        return Err(io::Error::new(io::ErrorKind::NotFound, msg));
    }
    let manifest_path_parent = manifest_path.parent().unwrap_or(curdir).canonicalize()?;
    let possible_lockfile = manifest_path_parent.join("Cargo.lock").canonicalize();
    let possible_lockfile = match possible_lockfile {
        Ok(canonicalized_path_to_lockfile) => canonicalized_path_to_lockfile,
        Err(_) => {
            warn!("Lockfile not found in path... will attempt to regenerate");
            cargo_generate_lockfile(&manifest_path_parent, manifest)?;
            manifest_path_parent.join("Cargo.lock").canonicalize()?
        }
    };

    if possible_lockfile.is_file() {
        if respect_lockfile {
            default_options.push("--locked".to_string());
        }
    } else {
        info!("🔓Attempting to regenerate lockfile...");
        cargo_generate_lockfile(curdir, manifest)?;
        info!("🔒Regenerated lockfile.");
    }
    // TARGET_TRIPLES.iter().for_each(|target| {
    //     default_options.push("--target".to_string());
    //     default_options.push(target.to_string());
    // });
    let res = cargo_command("fetch", &default_options, curdir);
    res.inspect(|_| {
        info!("✅ `cargo fetch` finished!");
    })
    .inspect_err(|err| {
        error!(?err);
    })
}

#[allow(clippy::too_many_arguments)]
pub fn cargo_vendor(
    custom_root: &Path,
    versioned_dirs: bool,
    filter: bool,
    manifest_paths: &[PathBuf],
    i_accept_the_risk: &[String],
    update: bool,
    crates: &[String],
    respect_lockfile: bool,
    vendor_dirname: &str,
) -> io::Result<Option<(PathBuf, String, bool)>> {
    let which_subcommand = if filter { "vendor-filterer" } else { "vendor" };
    let mut default_options: Vec<String> = vec![];
    if versioned_dirs {
        default_options.push("--versioned-dirs".to_string());
    }
    let mut first_manifest = custom_root.join("Cargo.toml");
    let mut lockfiles: Vec<PathBuf> = Vec::new();
    let mut global_has_deps = false;

    if !first_manifest.is_file() {
        let msg = format!(
            "⚠️  There seems to be no manifest at this path `{}`.",
            first_manifest.display()
        );
        warn!(msg, ?first_manifest);
        warn!("⚠️  Root manifest does not exist. Will attempt to fallback to manifest paths.");
        if let Some(first) = manifest_paths.first() {
            let fallback_manifest = custom_root.join(first);
            info!(?fallback_manifest, "🐥 Fallback root manifest found.");
            if fallback_manifest.exists() {
                first_manifest = fallback_manifest.to_path_buf();
            } else {
                return Err(io::Error::new(
                    io::ErrorKind::NotFound,
                    "No manifest in this path.",
                ));
            }
        } else {
            let msg = "Failed to vendor as their are no manifest files to use.";
            error!(msg, ?manifest_paths);
            return Err(io::Error::new(io::ErrorKind::NotFound, msg));
        };
    }

    info!("Using manifest {} for vendoring", first_manifest.display());

    let first_manifest_parent = first_manifest
        .parent()
        .unwrap_or(custom_root)
        .canonicalize()?;

    let possible_lockfile = first_manifest_parent.join("Cargo.lock").canonicalize();
    let possible_lockfile = match possible_lockfile {
        Ok(canonicalized_path_to_lockfile) => canonicalized_path_to_lockfile,
        Err(_) => {
            warn!("Lockfile not found in path... will attempt to regenerate");
            cargo_generate_lockfile(&first_manifest_parent, &first_manifest.to_string_lossy())?;
            first_manifest_parent.join("Cargo.lock").canonicalize()?
        }
    };

    let possible_vendordir = first_manifest_parent.join(vendor_dirname);
    if possible_vendordir.exists() {
        error!("Refusing to proceed - a vendor directory already exists in the source tar.");
        error!("You MUST specify a unique `tag` to proceed");
        return Err(io::Error::new(
            io::ErrorKind::DirectoryNotEmpty,
            "Refusing to proceed - vendor directory already exists. You must specify a unique tag.",
        ));
    }

    let mut hash = blake3::Hasher::new();

    if possible_lockfile.is_file() {
        let lockfile_bytes = fs::read(&possible_lockfile)?;
        hash.update(&lockfile_bytes);
        let output_hash = hash.finalize();
        debug!(?output_hash, "🔒 Lockfile hash before: ");
    }

    let is_manifest_workspace = is_workspace(&first_manifest)?;
    let has_deps = has_dependencies(&first_manifest)?;

    if is_manifest_workspace {
        info!("ℹ️  This manifest is in WORKSPACE configuration.");
        let workspace_has_deps = workspace_has_dependencies(custom_root, &first_manifest)?;
        if !workspace_has_deps {
            warn!(
                "⚠️ This WORKSPACE MANIFEST does not seem to contain workspace dependencies and dev-dependencies. Please check member dependencies."
            );
        }
        global_has_deps = global_has_deps || workspace_has_deps;
    } else if !has_deps {
        info!("😄 This manifest does not seem to have any dependencies.");
        info!(
            "🙂 If you think this is a BUG 🐞, please open an issue at <https://github.com/openSUSE-Rust/obs-service-cargo/issues>."
        );
    }

    global_has_deps = global_has_deps || has_deps;

    manifest_paths.iter().try_for_each(|manifest| {
        let extra_full_manifest_path = custom_root.join(manifest).canonicalize()?;
        if extra_full_manifest_path.exists() {
            let is_manifest_workspace = is_workspace(&extra_full_manifest_path)?;
            let has_deps = has_dependencies(&extra_full_manifest_path)?;
            if is_manifest_workspace {
                info!(?extra_full_manifest_path, "ℹ️ This manifest is in WORKSPACE configuration.");
                let workspace_has_deps = workspace_has_dependencies(custom_root, &first_manifest)?;
                if !workspace_has_deps {
                    warn!("⚠️ This WORKSPACE MANIFEST does not seem to contain workspace dependencies and dev-dependencies. Please check member dependencies.");
                }
                global_has_deps = global_has_deps || workspace_has_deps;
            } else if !has_deps {
                info!("😄 This manifest does not seem to have any dependencies.");
                info!("🙂 If you think this is a BUG 🐞, please open an issue at <https://github.com/openSUSE-Rust/obs-service-cargo/issues>.");
            };
            default_options.push("--sync".to_string());
            default_options.push(extra_full_manifest_path.to_string_lossy().to_string());
        } else {
            let msg = "Manifest path does not exist. Aborting operation.";
            error!(?extra_full_manifest_path, msg);
            return Err(io::Error::new(io::ErrorKind::NotFound, msg));
        }
        Ok(())
    })?;

    if possible_lockfile.is_file() {
        if filter {
            warn!(
                "⚠️ Vendor filterer does not support lockfile verification. Your dependencies MIGHT get updated."
            );
        } else if !filter && respect_lockfile {
            default_options.push("--locked".to_string());
        }

        debug!(?possible_lockfile, "🔓 Adding lockfile.");
        lockfiles.push(possible_lockfile.as_path().to_path_buf());
    } else {
        warn!(
            "⚠️ No lockfile present. This might UPDATE your dependency. Overriding `update` from \
				 false to true."
        );
        info!("🔓Attempting to regenerate lockfile...");
        cargo_generate_lockfile(&first_manifest_parent, &first_manifest.to_string_lossy())?;
        info!("🔒Regenerated lockfile.");
    }

    if filter {
        default_options.push("--platform=*-unknown-linux-gnu".to_string());
        default_options.push("--platform=wasm32-unknown-unknown".to_string());
        // NOTE: by <https://github.com/msirringhaus>
        // We are conservative here and vendor all possible features, even
        // if they are not used in the spec. But we can't know.
        // Maybe make this configurable?
        // NOTE to that NOTE: by uncomfyhalomacro
        // I think we won't because we can't guess every feature they have.
        // It's usually enabled on `cargo build -F` tbh...
        default_options.push("--all-features".to_string());
    }

    // Finally set the vendor path.
    default_options.push(vendor_dirname.to_string());

    if !update {
        warn!("😥 Disabled update of dependencies. You should enable this for security updates.");
    }

    cargo_update(
        update,
        crates,
        &first_manifest_parent,
        &first_manifest.to_string_lossy(),
        respect_lockfile,
    )?;

    cargo_fetch(
        &first_manifest_parent,
        &first_manifest.to_string_lossy(),
        respect_lockfile,
    )?;

    info!("🏪 Running `cargo {}`...", &which_subcommand);
    let res = cargo_command(which_subcommand, &default_options, first_manifest_parent);
    info!("💼 Vendor complete.");

    if possible_lockfile.is_file() {
        let lockfile_bytes = fs::read(&possible_lockfile)?;
        hash.update(&lockfile_bytes);
        let output_hash = hash.finalize();
        debug!(?output_hash, "🔒 Lockfile hash after: ");
        debug!(?possible_lockfile, "🔓 Adding lockfile.");
        lockfiles.push(possible_lockfile.as_path().to_path_buf());
    }

    info!("🛡️🫥 Auditing lockfiles...");
    if let Ok(audit_result) = audit::perform_cargo_audit(&lockfiles, i_accept_the_risk) {
        audit::process_reports(audit_result).map_err(|err| {
            error!(?err);
            io::Error::new(io::ErrorKind::Interrupted, err.to_string())
        })?;
    }
    info!("🛡️🙂 All lockfiles are audited");

    match res {
        Ok(output_cargo_configuration) => {
            if !global_has_deps {
                debug!(
                    "🎉 No dependencies! Still, we need to regenerate the lockfile to ensure cargo works."
                );
            }
            debug!("🏪 `cargo {}` finished.", &which_subcommand);
            Ok(Some((
                possible_lockfile
                    .canonicalize()
                    .unwrap_or(possible_lockfile),
                output_cargo_configuration,
                global_has_deps,
            )))
        }
        Err(err) => {
            error!(?err);
            Err(err)
        }
    }
}

pub fn cargo_generate_lockfile(curdir: &Path, manifest: &str) -> io::Result<String> {
    info!("🔓 💂 Running `cargo generate-lockfile`...");
    let mut original_hasher = blake3::Hasher::new();
    let mut regenerated_hasher = blake3::Hasher::new();
    let mut default_options: Vec<String> = vec![];
    let manifest_path = PathBuf::from(&manifest);
    let manifest_path_parent = manifest_path.parent().unwrap_or(curdir);
    let possible_lockfile = manifest_path_parent.join("Cargo.lock");

    if possible_lockfile.is_file() {
        let lockfile_bytes = fs::read(&possible_lockfile)?;
        original_hasher.update(&lockfile_bytes);
    } else {
        warn!("⚠️ No lockfile present. This **MIGHT** UPDATE your dependency.");
    }

    if !manifest.is_empty() {
        default_options.push("--manifest-path".to_string());
        default_options.push(manifest.to_string());
    }
    let res = cargo_command("generate-lockfile", &default_options, curdir);
    if possible_lockfile.exists() {
        let lockfile_bytes = fs::read(&possible_lockfile)?;
        regenerated_hasher.update(&lockfile_bytes);
    }
    let original_hash = original_hasher.finalize();
    let regenerated_hash = regenerated_hasher.finalize();
    debug!(?original_hash, ?regenerated_hash);
    if original_hash != regenerated_hash {
        warn!("⚠️ New lockfile generated");
        warn!(?regenerated_hash, "Lockfile hash");
    }
    res.inspect(|_| {
        info!("🔓 💂 `cargo generate-lockfile` finished.");
    })
    .inspect_err(|err| {
        error!(?err);
    })
}

pub fn cargo_update(
    global_update: bool,
    crates: &[String],
    curdir: &Path,
    manifest: &str,
    respect_lockfile: bool,
) -> io::Result<String> {
    let mut default_options: Vec<String> = vec![];
    if global_update {
        info!("⏫ Updating dependencies...");
        let manifest_path = PathBuf::from(&manifest).canonicalize()?;
        let manifest_path_parent = manifest_path.parent().unwrap_or(curdir);
        let possible_lockfile = manifest_path_parent.join("Cargo.lock").canonicalize();
        let possible_lockfile = match possible_lockfile {
            Ok(canonicalized_path_to_lockfile) => canonicalized_path_to_lockfile,
            Err(_) => {
                warn!("Lockfile not found in path... will attempt to regenerate");
                cargo_generate_lockfile(manifest_path_parent, manifest)?;
                manifest_path_parent.join("Cargo.lock").canonicalize()?
            }
        };

        if !manifest.is_empty() {
            default_options.push("--manifest-path".to_string());
            default_options.push(manifest.to_string());
        }

        if possible_lockfile.is_file() && respect_lockfile {
            default_options.push("--locked".to_string());
        }

        cargo_command("update", &default_options, curdir)
            .inspect(|_| {
                info!("✅ Updated dependencies.");
            })
            .inspect_err(|err| {
                error!(?err);
            })
    } else
    // If global update is false, it's possible that crates variable is not empty
    // and user might have specified specific crates to update
    if !crates.is_empty() {
        for crate_ in crates.iter() {
            let mut new_cur_dir = curdir.to_path_buf();
            if let Some((crate_name, string_tail)) = crate_.split_once("@") {
                info!(
                    "🦀 Applying update for specified crate dependency {}.",
                    crate_name
                );
                default_options.push(crate_name.to_string());
                if !string_tail.is_empty() {
                    if let Some((crate_ver, dependent)) = string_tail.split_once("+") {
                        if !crate_ver.trim().is_empty() {
                            if *crate_ver == *"recursive" {
                                info!(
                                    "📦🔄 Applying recursive update for crate dependency {}",
                                    crate_name
                                );
                                default_options.push("--recursive".to_string());
                            } else if semver::Version::parse(crate_ver)
                                .map_err(|err| {
                                    error!(?err);
                                    let msg =
                                        format!("Expected a valid version string. Got {crate_ver}");
                                    io::Error::new(io::ErrorKind::InvalidInput, msg)
                                })
                                .is_ok()
                            {
                                info!(
                                    "📦🥄 Applying precise update for crate dependency {} to version {}",
                                    crate_name, crate_ver
                                );
                                default_options.push("--precise".to_string());
                                default_options.push(crate_ver.to_string());
                            } else {
                                let msg = format!(
                                    "Expected a valid `cargo update` option for {crate_name}. Got {crate_ver}"
                                );
                                return Err(io::Error::new(io::ErrorKind::InvalidInput, msg));
                            }
                        }

                        if !dependent.trim().is_empty() {
                            if !dependent.ends_with("Cargo.toml") {
                                let msg = format!(
                                    "Expected a valid manifest filename. Got {dependent}.",
                                );
                                error!(?dependent, msg);
                                return Err(io::Error::new(io::ErrorKind::InvalidInput, msg));
                            }
                            info!("🏗️ Updating {} at {}.", crate_name, dependent);
                            let dependent_manifest_path = curdir.join(dependent).canonicalize()?;
                            default_options.push("--manifest-path".to_string());
                            default_options
                                .push(dependent_manifest_path.to_string_lossy().to_string());
                            let manifest_path_parent =
                                dependent_manifest_path.parent().unwrap_or(curdir);
                            new_cur_dir = manifest_path_parent.to_path_buf();
                            let possible_lockfile = manifest_path_parent.join("Cargo.lock");
                            if possible_lockfile.is_file() && respect_lockfile {
                                default_options.push("--locked".to_string());
                            }
                        }
                    } else {
                        // NOTE: string_tail then is now our crate version
                        if *string_tail == *"recursive" {
                            info!(
                                "📦🔄 Applying recursive update for crate dependency {}",
                                crate_name
                            );
                            default_options.push("--recursive".to_string());
                        } else if semver::Version::parse(string_tail)
                            .map_err(|err| {
                                error!(?err);
                                let msg =
                                    format!("Expected a valid version string. Got {string_tail}");
                                io::Error::new(io::ErrorKind::InvalidInput, msg)
                            })
                            .is_ok()
                        {
                            info!(
                                "📦🥄 Applying precise update for crate dependency {} to version {}",
                                crate_name, string_tail
                            );
                            default_options.push("--precise".to_string());
                            default_options.push(string_tail.to_string());
                        } else {
                            let msg = format!(
                                "Expected a valid `cargo update` option for {crate_name}. Got {string_tail}"
                            );
                            return Err(io::Error::new(io::ErrorKind::InvalidInput, msg));
                        }
                    }
                }
            // NOTE: `+` can be first then `@` second.
            } else if let Some((crate_name, string_tail)) = crate_.split_once("+") {
                default_options.push(crate_name.to_string());
                if !string_tail.is_empty() {
                    if let Some((dependent, crate_ver)) = string_tail.split_once("@") {
                        if !crate_ver.trim().is_empty() {
                            if *crate_ver == *"recursive" {
                                info!(
                                    "📦🔄 Applying recursive update for crate dependency {}",
                                    crate_name
                                );
                                default_options.push("--recursive".to_string());
                            } else if semver::Version::parse(crate_ver)
                                .map_err(|err| {
                                    error!(?err);
                                    let msg =
                                        format!("Expected a valid version string. Got {crate_ver}");
                                    io::Error::new(io::ErrorKind::InvalidInput, msg)
                                })
                                .is_ok()
                            {
                                info!(
                                    "📦🥄 Applying precise update for crate dependency {} to version {}",
                                    crate_name, crate_ver
                                );
                                default_options.push("--precise".to_string());
                                default_options.push(crate_ver.to_string());
                            } else {
                                let msg = format!(
                                    "Expected a valid `cargo update` option for {crate_name}. Got {crate_ver}"
                                );
                                return Err(io::Error::new(io::ErrorKind::InvalidInput, msg));
                            }
                        }

                        if !dependent.trim().is_empty() {
                            if !dependent.ends_with("Cargo.toml") {
                                let msg = format!(
                                    "Expected a valid manifest filename. Got {dependent}.",
                                );
                                error!(?dependent, msg);
                                return Err(io::Error::new(io::ErrorKind::InvalidInput, msg));
                            }
                            info!("🏗️ Updating {} at {}.", crate_name, dependent);
                            let dependent_manifest_path = curdir.join(dependent).canonicalize()?;
                            default_options.push("--manifest-path".to_string());
                            default_options
                                .push(dependent_manifest_path.to_string_lossy().to_string());
                            let manifest_path_parent =
                                dependent_manifest_path.parent().unwrap_or(curdir);
                            let possible_lockfile = manifest_path_parent.join("Cargo.lock");
                            new_cur_dir = manifest_path_parent.to_path_buf();
                            if possible_lockfile.is_file() && respect_lockfile {
                                default_options.push("--locked".to_string());
                            }
                        }
                    } else {
                        // NOTE: string_tail is now our dependent crate here.
                        if !string_tail.trim().is_empty() {
                            if !string_tail.ends_with("Cargo.toml") {
                                let msg = format!(
                                    "Expected a valid manifest filename. Got {string_tail}.",
                                );
                                error!(?string_tail, msg);
                                return Err(io::Error::new(io::ErrorKind::InvalidInput, msg));
                            }
                            info!("🏗️ Updating {} at {}.", crate_name, string_tail);
                            let string_tail_manifest_path =
                                curdir.join(string_tail).canonicalize()?;
                            default_options.push("--manifest-path".to_string());
                            default_options
                                .push(string_tail_manifest_path.to_string_lossy().to_string());
                            let manifest_path_parent =
                                string_tail_manifest_path.parent().unwrap_or(curdir);
                            let possible_lockfile = manifest_path_parent.join("Cargo.lock");
                            new_cur_dir = manifest_path_parent.to_path_buf();
                            if possible_lockfile.is_file() && respect_lockfile {
                                default_options.push("--locked".to_string());
                            }
                        }
                    }
                }
            }
            cargo_command("update", &default_options, new_cur_dir)
                .inspect(|_| {
                    info!("✅ Updated dependencies for crate.");
                })
                .inspect_err(|err| {
                    error!(?err);
                    // There is no point of handling error if a PKGID or crate does not exist for a particular manifest path
                    // because at the end of the day, if two manifest paths do have the same crate that was specified to update
                    // then the one in the registry or vendor gets updated with the same version as well.
                    // NOTE: Maybe in the future we can add ways to be specific on each manifest path
                    warn!("This error will be ignored.");
                })?;
            default_options = Vec::new();
        }
        let success_msg = "ℹ️ Finished updating specified crate dependencies.".to_string();
        Ok(success_msg)
    } else {
        let msg = "🫠 Nothing to update.".to_string();
        info!("{}", &msg);
        Ok(msg)
    }
}
